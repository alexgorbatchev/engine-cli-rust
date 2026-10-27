use super::{
    Database, Error, Result, Track, identifier, read_track, require_changed, timestamp, unique,
};
use rusqlite::OptionalExtension;
use std::path::Path;

impl Database {
    pub fn tracks(&self) -> Result<Vec<Track>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {} FROM Track ORDER BY id",
            self.selection()
        ))?;
        Ok(statement
            .query_map([], read_track)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn track(&self, id: i64) -> Result<Option<Track>> {
        Ok(self
            .connection
            .query_row(
                &format!("SELECT {} FROM Track WHERE id=?", self.selection()),
                [id],
                read_track,
            )
            .optional()?)
    }

    pub fn find_track(&self, query: &str) -> Result<Option<Track>> {
        let query = identifier(query)?;
        if let Ok(id) = query.parse::<i64>()
            && let Some(track) = self.track(id)?
        {
            return Ok(Some(track));
        }
        let tracks = self.tracks()?;
        let exact: Vec<_> = tracks
            .iter()
            .filter(|track| {
                track.path.as_deref() == Some(query)
                    || track.filename.as_deref() == Some(query)
                    || track
                        .title
                        .as_deref()
                        .is_some_and(|title| title.eq_ignore_ascii_case(query))
            })
            .cloned()
            .collect();
        if !exact.is_empty() {
            return unique("track", query, exact, |track| track.id);
        }
        unique(
            "track",
            query,
            tracks
                .into_iter()
                .filter(|track| {
                    track
                        .path
                        .as_ref()
                        .is_some_and(|path| path.starts_with(query) || path.ends_with(query))
                })
                .collect(),
            |track| track.id,
        )
    }

    pub fn insert_track(&self, track: &mut Track) -> Result<i64> {
        track.last_edit_time = Some(timestamp());
        let pairs = self.writable_track(track);
        let values: Vec<_> = pairs.iter().map(|(_, v)| *v).collect();
        let names = pairs
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(",");
        let placeholders = vec!["?"; values.len()].join(",");
        self.connection.execute(
            &format!("INSERT INTO Track ({names}) VALUES ({placeholders})"),
            rusqlite::params_from_iter(values),
        )?;
        track.id = self.connection.last_insert_rowid();
        Ok(track.id)
    }

    pub fn update_track(&self, track: &Track) -> Result<()> {
        let mut current = track.clone();
        current.last_edit_time = Some(timestamp());
        let pairs = self.writable_track(&current);
        let mut values: Vec<_> = pairs.iter().map(|(_, v)| *v).collect();
        let names = pairs
            .iter()
            .map(|(name, _)| format!("{name}=?"))
            .collect::<Vec<_>>()
            .join(",");
        values.push(&track.id);
        let changed = self.connection.execute(
            &format!("UPDATE Track SET {names} WHERE id=?"),
            rusqlite::params_from_iter(values),
        )?;
        require_changed(changed, format!("track {}", track.id))
    }

    pub fn delete_track(&self, id: i64) -> Result<()> {
        self.transaction(|database| {
            database
                .track(id)?
                .ok_or_else(|| Error::NotFound(format!("track {id}")))?;
            let lists = database
                .connection
                .prepare("SELECT DISTINCT listId FROM PlaylistEntity WHERE trackId=?")?
                .query_map([id], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for list in lists {
                database.remove_membership_inner(list, id)?;
            }
            if !super::columns(&database.connection, "PerformanceData")?.is_empty() {
                database
                    .connection
                    .execute("DELETE FROM PerformanceData WHERE trackId=?", [id])?;
            }
            database
                .connection
                .execute("DELETE FROM Track WHERE id=?", [id])?;
            Ok(())
        })
    }
    pub fn relocate_tracks(&self, from: &str, to: &str) -> Result<usize> {
        if from.is_empty() {
            return Err(Error::Invalid("old directory prefix is empty".into()));
        }
        self.transaction(|database| {
            let mut changed = 0;
            for mut track in database.tracks()? {
                let Some(path) = track.path.as_ref().map(Path::new) else {
                    continue;
                };
                if let Ok(relative) = path.strip_prefix(from) {
                    let path = Path::new(to).join(relative);
                    track.filename = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    track.path = Some(
                        path.to_str()
                            .ok_or_else(|| Error::Invalid("SQLite path is not UTF-8".into()))?
                            .into(),
                    );
                    database.update_track(&track)?;
                    changed += 1;
                }
            }
            Ok(changed)
        })
    }
}
