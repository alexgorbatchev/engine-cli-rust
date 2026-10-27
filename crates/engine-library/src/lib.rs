//! Native Engine DJ collection services. Opening a database never creates or migrates its schema.
pub mod artwork;
pub mod backup;
pub mod collection;
pub mod db;
pub mod history;
pub mod model;
pub mod music;
pub mod performance;
pub mod records;

pub use db::Database;
pub use model::{AlbumArt, Playlist, PlaylistEntity, Track};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("ambiguous {kind} {query:?}; candidate IDs: {ids:?}")]
    Ambiguous {
        kind: &'static str,
        query: String,
        ids: Vec<i64>,
    },
    #[error("collection integrity: {0}")]
    Integrity(String),
    #[error("unsupported collection schema: {0}")]
    Schema(String),
    #[error("invalid Engine performance data: {0}")]
    Codec(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("restore stopped after updating {completed:?}: {source}")]
    PartialRestore {
        completed: Vec<std::path::PathBuf>,
        #[source]
        source: Box<Error>,
    },
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Audio(audio_metadata::Error),
    #[error("database committed, but cleanup of {path} failed: {source}")]
    CommittedCleanup {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{primary}; rollback cleanup of {path} also failed: {source}")]
    Cleanup {
        primary: Box<Error>,
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl From<audio_metadata::Error> for Error {
    fn from(error: audio_metadata::Error) -> Self {
        if matches!(error, audio_metadata::Error::Cancelled) {
            Self::Cancelled
        } else {
            Self::Audio(error)
        }
    }
}
