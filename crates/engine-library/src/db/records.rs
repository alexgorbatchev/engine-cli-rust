use super::{Database, TRACK_COLUMNS, columns, track_values};
use crate::{
    Error, Result,
    records::{Information, PlaylistMetadata, Smartlist, TrackMetadata, TrackRecord},
};
use rusqlite::{Connection, OptionalExtension, ToSql, params};
use std::path::Path;

impl Database {
    /// Create a new database atomically; existing destinations are never modified.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Self::create_inner(path.as_ref(), false)
    }
    /// Create a history database with native track identity and history-event rows.
    pub fn create_history(path: impl AsRef<Path>) -> Result<Self> {
        Self::create_inner(path.as_ref(), true)
    }
    fn create_inner(path: &Path, history: bool) -> Result<Self> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        let connection = Connection::open(temporary.path())?;
        connection.execute_batch(include_str!("schema.sql"))?;
        if history {
            connection.execute_batch("CREATE TABLE HistorylistEntity(id INTEGER PRIMARY KEY AUTOINCREMENT,trackId INTEGER,startTime DATETIME);")?;
        }
        let database = Self::from_connection(connection)?;
        database.put_information(&Information {
            id: 1,
            uuid: uuid::Uuid::new_v4(),
            schema_version: [2, 20, 0],
            current_played_indicator: 0,
            last_rekordbox_library_import_read_counter: 0,
        })?;
        drop(database);
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
        Self::open(path, false)
    }

    pub fn track_record(&self, id: i64) -> Result<Option<TrackRecord>> {
        let Some(track) = self.track(id)? else {
            return Ok(None);
        };
        let projection = project(&self.connection, "Track", TrackMetadata::COLUMNS)?;
        let metadata = self.connection.query_row(
            &format!("SELECT {projection} FROM Track WHERE id=?"),
            [id],
            TrackMetadata::read,
        )?;
        Ok(Some(TrackRecord { track, metadata }))
    }
    pub fn track_records(&self) -> Result<Vec<TrackRecord>> {
        self.tracks()?
            .into_iter()
            .map(|track| {
                self.track_record(track.id)?
                    .ok_or_else(|| Error::Integrity("track disappeared during record read".into()))
            })
            .collect()
    }
    /// Import a complete record with its explicit identity and edit time, without schema migration.
    pub fn put_track_record(&self, record: &TrackRecord) -> Result<()> {
        if record.track.id <= 0 {
            return Err(Error::Invalid("track ID must be positive".into()));
        }
        let available = columns(&self.connection, "Track")?;
        let mut pairs: Vec<(&str, &dyn ToSql)> = vec![("id", &record.track.id)];
        pairs.extend(
            TRACK_COLUMNS
                .split(',')
                .skip(1)
                .chain(self.source_hash.then_some("albumArtSourceHash"))
                .zip(track_values(&record.track, self.source_hash)),
        );
        pairs.extend(record.metadata.values());
        pairs.retain(|(name, _)| available.contains(*name));
        upsert(&self.connection, "Track", "id", &pairs)
    }
    pub fn playlist_metadata(&self, id: i64) -> Result<PlaylistMetadata> {
        let projection = project(&self.connection, "Playlist", PlaylistMetadata::COLUMNS)?;
        Ok(self.connection.query_row(
            &format!("SELECT {projection} FROM Playlist WHERE id=?"),
            [id],
            PlaylistMetadata::read,
        )?)
    }
    pub fn put_playlist_metadata(&self, id: i64, metadata: &PlaylistMetadata) -> Result<()> {
        self.require_playlist(id)?;
        let available = columns(&self.connection, "Playlist")?;
        let pairs: Vec<_> = metadata
            .values()
            .into_iter()
            .filter(|(name, _)| available.contains(*name))
            .collect();
        if pairs.is_empty() {
            return Ok(());
        }
        let setters = pairs
            .iter()
            .map(|(name, _)| format!("{name}=?"))
            .collect::<Vec<_>>()
            .join(",");
        let mut values: Vec<_> = pairs.iter().map(|(_, value)| *value).collect();
        values.push(&id);
        self.connection.execute(
            &format!("UPDATE Playlist SET {setters} WHERE id=?"),
            rusqlite::params_from_iter(values),
        )?;
        Ok(())
    }
    pub fn information(&self) -> Result<Option<Information>> {
        if columns(&self.connection, "Information")?.is_empty() {
            return Ok(None);
        }
        let raw=self.connection.query_row("SELECT id,uuid,schemaVersionMajor,schemaVersionMinor,schemaVersionPatch,COALESCE(currentPlayedIndiciator,0),COALESCE(lastRekordBoxLibraryImportReadCounter,0) FROM Information ORDER BY id LIMIT 1",[],|row|Ok((row.get(0)?,row.get::<_,String>(1)?,[row.get(2)?,row.get(3)?,row.get(4)?],row.get(5)?,row.get(6)?))).optional()?;
        raw.map(
            |(
                id,
                uuid,
                schema_version,
                current_played_indicator,
                last_rekordbox_library_import_read_counter,
            )| {
                Ok(Information {
                    id,
                    uuid: uuid::Uuid::parse_str(&uuid).map_err(|error| {
                        Error::Integrity(format!("invalid database UUID: {error}"))
                    })?,
                    schema_version,
                    current_played_indicator,
                    last_rekordbox_library_import_read_counter,
                })
            },
        )
        .transpose()
    }
    pub fn put_information(&self, info: &Information) -> Result<()> {
        self.connection.execute("INSERT INTO Information(id,uuid,schemaVersionMajor,schemaVersionMinor,schemaVersionPatch,currentPlayedIndiciator,lastRekordBoxLibraryImportReadCounter) VALUES (?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET uuid=excluded.uuid,schemaVersionMajor=excluded.schemaVersionMajor,schemaVersionMinor=excluded.schemaVersionMinor,schemaVersionPatch=excluded.schemaVersionPatch,currentPlayedIndiciator=excluded.currentPlayedIndiciator,lastRekordBoxLibraryImportReadCounter=excluded.lastRekordBoxLibraryImportReadCounter",params![info.id,info.uuid.to_string(),info.schema_version[0],info.schema_version[1],info.schema_version[2],info.current_played_indicator,info.last_rekordbox_library_import_read_counter])?;
        Ok(())
    }
    pub fn smartlists(&self) -> Result<Vec<Smartlist>> {
        if columns(&self.connection, "Smartlist")?.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.connection.prepare("SELECT listUuid,title,parentPlaylistPath,nextPlaylistPath,nextListUuid,rules,CAST(lastEditTime AS TEXT) FROM Smartlist ORDER BY listUuid")?.query_map([],|row|Ok(Smartlist {list_uuid:row.get(0)?,title:row.get(1)?,parent_playlist_path:row.get(2)?,next_playlist_path:row.get(3)?,next_list_uuid:row.get(4)?,rules:row.get(5)?,last_edit_time:row.get(6)?}))?.collect::<rusqlite::Result<_>>()?)
    }
    pub fn put_smartlist(&self, record: &Smartlist) -> Result<()> {
        if record.list_uuid.is_empty() {
            return Err(Error::Invalid("empty smartlist UUID".into()));
        }
        upsert(
            &self.connection,
            "Smartlist",
            "listUuid",
            &[
                ("listUuid", &record.list_uuid),
                ("title", &record.title),
                ("parentPlaylistPath", &record.parent_playlist_path),
                ("nextPlaylistPath", &record.next_playlist_path),
                ("nextListUuid", &record.next_list_uuid),
                ("rules", &record.rules),
                ("lastEditTime", &record.last_edit_time),
            ],
        )
    }
}

fn project(connection: &Connection, table: &str, names: &[&str]) -> Result<String> {
    let available = columns(connection, table)?;
    Ok(names
        .iter()
        .map(|name| {
            if !available.contains(*name) {
                format!("NULL AS {name}")
            } else if name.contains("Time") || name.starts_with("date") {
                format!("CAST({name} AS TEXT) AS {name}")
            } else {
                (*name).into()
            }
        })
        .collect::<Vec<_>>()
        .join(","))
}
pub(super) fn upsert(
    connection: &Connection,
    table: &str,
    key: &str,
    pairs: &[(&str, &dyn ToSql)],
) -> Result<()> {
    let names = pairs
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(",");
    let placeholders = vec!["?"; pairs.len()].join(",");
    let assignments = pairs
        .iter()
        .filter(|(name, _)| *name != key)
        .map(|(name, _)| format!("{name}=excluded.{name}"))
        .collect::<Vec<_>>()
        .join(",");
    connection.execute(&format!("INSERT INTO {table}({names}) VALUES ({placeholders}) ON CONFLICT({key}) DO UPDATE SET {assignments}"),rusqlite::params_from_iter(pairs.iter().map(|(_,value)|*value)))?;
    Ok(())
}
