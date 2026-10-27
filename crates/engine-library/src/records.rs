//! Complete collection records, complementing the lightweight track/playlist CRUD views.
use crate::Track;
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Serialize};

macro_rules! metadata {
    ($name:ident { $($field:ident : $ty:ty => $column:literal),* $(,)? }) => {
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        pub struct $name { $(pub $field:Option<$ty>,)* }
        impl $name {
            pub(crate) const COLUMNS:&'static [&'static str]=&[$($column,)*];
            pub(crate) fn read(row:&Row<'_>)->rusqlite::Result<Self> {
                Ok(Self { $($field:row.get($column)?,)* })
            }
            pub(crate) fn values(&self)->Vec<(&'static str,&dyn ToSql)> {
                vec![$(($column,&self.$field as &dyn ToSql),)*]
            }
        }
    }
}

metadata!(TrackMetadata {
    time_last_played:String=>"timeLastPlayed",
    is_played:bool=>"isPlayed",
    date_created:String=>"dateCreated",
    date_added:String=>"dateAdded",
    is_metadata_of_packed_track_changed:bool=>"isMetadataOfPackedTrackChanged",
    is_performance_data_of_packed_track_changed:bool=>"isPerfomanceDataOfPackedTrackChanged",
    played_indicator:i64=>"playedIndicator",
    is_metadata_imported:bool=>"isMetadataImported",
    pdb_import_key:i64=>"pdbImportKey",
    streaming_source:String=>"streamingSource",
    uri:String=>"uri",
    is_beat_grid_locked:bool=>"isBeatGridLocked",
    origin_database_uuid:String=>"originDatabaseUuid",
    origin_track_id:i64=>"originTrackId",
    streaming_flags:i64=>"streamingFlags",
    explicit_lyrics:bool=>"explicitLyrics",
});

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackRecord {
    pub track: Track,
    pub metadata: TrackMetadata,
}

metadata!(PlaylistMetadata {
    is_persisted:bool=>"isPersisted",
    next_list_id:i64=>"nextListId",
    last_edit_time:String=>"lastEditTime",
    is_explicitly_exported:bool=>"isExplicitlyExported",
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Information {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub schema_version: [u32; 3],
    pub current_played_indicator: i64,
    pub last_rekordbox_library_import_read_counter: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Smartlist {
    pub list_uuid: String,
    pub title: Option<String>,
    pub parent_playlist_path: Option<String>,
    pub next_playlist_path: Option<String>,
    pub next_list_uuid: Option<String>,
    pub rules: Option<String>,
    pub last_edit_time: Option<String>,
}

/// Parse the date representations emitted by inspected Engine databases.
pub fn parse_timestamp(value: &str) -> crate::Result<Option<chrono::DateTime<chrono::Utc>>> {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
    if value.is_empty() {
        return Ok(None);
    }
    if let Ok(seconds) = value.parse::<f64>() {
        if !seconds.is_finite() || seconds < i64::MIN as f64 || seconds >= i64::MAX as f64 {
            return Err(crate::Error::Invalid("timestamp is out of range".into()));
        }
        return DateTime::from_timestamp(seconds as i64, 0)
            .map(Some)
            .ok_or_else(|| crate::Error::Invalid("timestamp is out of range".into()));
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return Ok(Some(date.with_timezone(&Utc)));
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%.f%z"] {
        if let Ok(date) = DateTime::parse_from_str(value, format) {
            return Ok(Some(date.with_timezone(&Utc)));
        }
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(date) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(Some(date.and_utc()));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(Some(date.and_hms_opt(0, 0, 0).expect("midnight").and_utc()));
    }
    Err(crate::Error::Invalid(format!(
        "invalid timestamp {value:?}"
    )))
}
