use serde::{Deserialize, Serialize};

/// Import/display policy must be chosen from the collection's documented schema, never guessed from values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DurationUnit {
    Seconds,
    #[default]
    Milliseconds,
}

impl DurationUnit {
    pub fn seconds(self, length: i64) -> i64 {
        match self {
            Self::Seconds => length,
            Self::Milliseconds => length / 1000,
        }
    }
    pub fn from_seconds(self, duration: f64) -> Option<i64> {
        let value = duration
            * match self {
                Self::Seconds => 1.0,
                Self::Milliseconds => 1000.0,
            };
        (value.is_finite() && value > 0.0 && value < i64::MAX as f64).then_some(value as i64)
    }
}

/// SQLite nullable metadata is represented by `Option`; length retains native database units.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub play_order: Option<i64>,
    pub length: Option<i64>,
    pub bpm: Option<i64>,
    pub year: Option<i64>,
    pub path: Option<String>,
    pub filename: Option<String>,
    pub bitrate: Option<i64>,
    pub bpm_analyzed: Option<f64>,
    pub album_art_id: Option<i64>,
    pub file_bytes: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub label: Option<String>,
    pub composer: Option<String>,
    pub remixer: Option<String>,
    pub key: Option<i64>,
    pub rating: Option<i64>,
    pub album_art: Option<String>,
    pub file_type: Option<String>,
    pub is_analyzed: Option<bool>,
    pub is_available: Option<bool>,
    pub last_edit_time: Option<String>,
    pub album_art_source_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub title: String,
    pub parent_list_id: i64,
    /// None means the database has no independent folder flag.
    pub is_folder: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntity {
    pub id: i64,
    pub list_id: i64,
    pub track_id: i64,
    pub next_entity_id: i64,
    pub membership_reference: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlbumArt {
    pub id: i64,
    pub hash: Option<String>,
    pub data: Vec<u8>,
}
