//! Read history using the original database and track identity, never a bare origin ID alone.
use crate::{Result, db::columns, records::parse_timestamp};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub origin_database_uuid: Option<String>,
    pub origin_track_id: i64,
    pub plays: u64,
    pub last_played: Option<DateTime<Utc>>,
}

/// Append a native history event for an existing track. Missing timestamps remain NULL.
pub fn append(connection: &Connection, track_id: i64, time: Option<DateTime<Utc>>) -> Result<i64> {
    let changed = connection.execute(
        "INSERT INTO HistorylistEntity(trackId,startTime) SELECT id,? FROM Track WHERE id=?",
        rusqlite::params![time.map(|date| date.to_rfc3339()), track_id],
    )?;
    if changed == 0 {
        return Err(crate::Error::NotFound(format!("track {track_id}")));
    }
    Ok(connection.last_insert_rowid())
}

pub fn read(connection: &Connection) -> Result<Vec<HistoryEntry>> {
    if columns(connection, "HistorylistEntity")?.is_empty() {
        return Ok(Vec::new());
    }
    let track_columns = columns(connection, "Track")?;
    let id = if track_columns.contains("originTrackId") {
        "COALESCE(NULLIF(t.originTrackId,0),t.id)"
    } else {
        "t.id"
    };
    let uuid = if track_columns.contains("originDatabaseUuid") {
        "t.originDatabaseUuid"
    } else {
        "NULL"
    };
    let mut statement=connection.prepare(&format!("SELECT {uuid},{id},CAST(h.startTime AS TEXT) FROM Track t JOIN HistorylistEntity h ON t.id=h.trackId"))?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut entries = BTreeMap::new();
    for row in rows {
        let (origin_database_uuid, origin_track_id, time) = row?;
        let played = time.as_deref().map(parse_timestamp).transpose()?.flatten();
        let entry = entries
            .entry((origin_database_uuid.clone(), origin_track_id))
            .or_insert(HistoryEntry {
                origin_database_uuid,
                origin_track_id,
                plays: 0,
                last_played: None,
            });
        entry.plays += 1;
        entry.last_played = entry.last_played.max(played);
    }
    Ok(entries.into_values().collect())
}
