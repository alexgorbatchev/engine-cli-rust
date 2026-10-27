mod artwork;
mod performance;
mod playlist_order;
mod playlists;
mod records;
mod tracks;

use std::{collections::BTreeSet, path::Path, time::Duration};

use rusqlite::{Connection, OpenFlags, Row, ToSql, Transaction, TransactionBehavior};

use crate::{AlbumArt, Error, Playlist, PlaylistEntity, Result, Track};

const TRACK_COLUMNS: &str = "id,playOrder,length,bpm,year,path,filename,bitrate,bpmAnalyzed,albumArtId,fileBytes,title,artist,album,genre,comment,label,composer,remixer,key,rating,albumArt,fileType,isAnalyzed,isAvailable,lastEditTime";

/// Owns a real SQLite connection; schema inspection never alters a collection.
pub struct Database {
    connection: Connection,
    source_hash: bool,
    folder_flag: bool,
    track_columns: BTreeSet<String>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>, readonly: bool) -> Result<Self> {
        let flags = if readonly {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        };
        Self::from_connection(Connection::open_with_flags(path, flags)?)
    }

    pub fn from_connection(connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        let track = columns(&connection, "Track")?;
        let playlist = columns(&connection, "Playlist")?;
        for column in TRACK_COLUMNS.split(',').take(22) {
            if !track.contains(column) {
                return Err(Error::Schema(format!("Track.{column} is absent")));
            }
        }
        for (table, required) in [
            ("Playlist", &["id", "title", "parentListId"][..]),
            (
                "PlaylistEntity",
                &[
                    "id",
                    "listId",
                    "trackId",
                    "nextEntityId",
                    "membershipReference",
                ][..],
            ),
            ("AlbumArt", &["id", "hash", "albumArt"][..]),
        ] {
            let existing = columns(&connection, table)?;
            for column in required {
                if !existing.contains(*column) {
                    return Err(Error::Schema(format!("{table}.{column} is absent")));
                }
            }
        }
        Ok(Self {
            source_hash: track.contains("albumArtSourceHash"),
            folder_flag: playlist.contains("isFolder"),
            track_columns: track,
            connection,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn has_folder_flag(&self) -> bool {
        self.folder_flag
    }

    pub fn transaction<T>(&self, action: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let value = action(self)?;
        transaction.commit()?;
        Ok(value)
    }

    fn selection(&self) -> String {
        format!(
            "{},{}",
            TRACK_COLUMNS
                .split(',')
                .map(|name| {
                    if !self.track_columns.contains(name) {
                        format!("NULL AS {name}")
                    } else if name == "lastEditTime" {
                        "CAST(lastEditTime AS TEXT) AS lastEditTime".into()
                    } else {
                        name.into()
                    }
                })
                .collect::<Vec<String>>()
                .join(","),
            if self.source_hash {
                "albumArtSourceHash"
            } else {
                "NULL AS albumArtSourceHash"
            }
        )
    }

    fn writable_track<'a>(&self, track: &'a Track) -> Vec<(&'static str, &'a dyn ToSql)> {
        TRACK_COLUMNS
            .split(',')
            .skip(1)
            .chain(self.source_hash.then_some("albumArtSourceHash"))
            .zip(track_values(track, self.source_hash))
            .filter(|(name, _)| self.track_columns.contains(*name))
            .collect()
    }
}

fn identifier(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::Invalid("empty identifier".into()));
    }
    Ok(value)
}

fn unique<T>(
    kind: &'static str,
    query: &str,
    mut items: Vec<T>,
    id: impl Fn(&T) -> i64,
) -> Result<Option<T>> {
    if items.len() > 1 {
        return Err(Error::Ambiguous {
            kind,
            query: query.into(),
            ids: items.iter().map(id).collect(),
        });
    }
    Ok(items.pop())
}

pub(crate) fn columns(connection: &Connection, table: &str) -> Result<BTreeSet<String>> {
    Ok(connection
        .prepare("SELECT name FROM pragma_table_info(?)")?
        .query_map([table], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn require_changed(changed: usize, what: String) -> Result<()> {
    if changed == 0 {
        return Err(Error::NotFound(what));
    }
    Ok(())
}

fn read_track(row: &Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        play_order: row.get(1)?,
        length: row.get(2)?,
        bpm: row.get(3)?,
        year: row.get(4)?,
        path: row.get(5)?,
        filename: row.get(6)?,
        bitrate: row.get(7)?,
        bpm_analyzed: row.get(8)?,
        album_art_id: row.get(9)?,
        file_bytes: row.get(10)?,
        title: row.get(11)?,
        artist: row.get(12)?,
        album: row.get(13)?,
        genre: row.get(14)?,
        comment: row.get(15)?,
        label: row.get(16)?,
        composer: row.get(17)?,
        remixer: row.get(18)?,
        key: row.get(19)?,
        rating: row.get(20)?,
        album_art: row.get(21)?,
        file_type: row.get(22)?,
        is_analyzed: row.get(23)?,
        is_available: row.get(24)?,
        last_edit_time: row.get(25)?,
        album_art_source_hash: row.get(26)?,
    })
}

fn track_values(track: &Track, source_hash: bool) -> Vec<&dyn ToSql> {
    let mut values: Vec<&dyn ToSql> = vec![
        &track.play_order,
        &track.length,
        &track.bpm,
        &track.year,
        &track.path,
        &track.filename,
        &track.bitrate,
        &track.bpm_analyzed,
        &track.album_art_id,
        &track.file_bytes,
        &track.title,
        &track.artist,
        &track.album,
        &track.genre,
        &track.comment,
        &track.label,
        &track.composer,
        &track.remixer,
        &track.key,
        &track.rating,
        &track.album_art,
        &track.file_type,
        &track.is_analyzed,
        &track.is_available,
        &track.last_edit_time,
    ];
    if source_hash {
        values.push(&track.album_art_source_hash);
    }
    values
}
