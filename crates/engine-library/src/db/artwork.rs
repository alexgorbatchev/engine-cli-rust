use super::{AlbumArt, Database, Result, require_changed};
use rusqlite::{OptionalExtension, params};

impl Database {
    /// Import artwork without remapping track references to a different identity.
    pub fn put_artwork(&self, art: &AlbumArt) -> Result<()> {
        if art.id <= 0 {
            return Err(super::Error::Invalid("artwork ID must be positive".into()));
        }
        super::records::upsert(
            &self.connection,
            "AlbumArt",
            "id",
            &[
                ("id", &art.id),
                ("hash", &art.hash),
                ("albumArt", &art.data),
            ],
        )
    }

    pub fn artwork(&self) -> Result<Vec<AlbumArt>> {
        let mut statement = self
            .connection
            .prepare("SELECT id,hash,COALESCE(albumArt,X'') FROM AlbumArt ORDER BY id")?;
        Ok(statement
            .query_map([], |row| {
                Ok(AlbumArt {
                    id: row.get(0)?,
                    hash: row.get(1)?,
                    data: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn upsert_art(&self, hash: &str, bytes: &[u8]) -> Result<i64> {
        let id = self
            .connection
            .query_row(
                "SELECT id FROM AlbumArt WHERE hash=? ORDER BY id LIMIT 1",
                [hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            self.update_art(id, hash, bytes)?;
            return Ok(id);
        }
        self.connection.execute(
            "INSERT INTO AlbumArt(hash,albumArt) VALUES (?,?)",
            params![hash, bytes],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn update_art(&self, id: i64, hash: &str, bytes: &[u8]) -> Result<()> {
        require_changed(
            self.connection.execute(
                "UPDATE AlbumArt SET hash=?,albumArt=? WHERE id=?",
                params![hash, bytes, id],
            )?,
            format!("artwork {id}"),
        )
    }

    pub fn delete_art(&self, id: i64) -> Result<()> {
        self.connection
            .execute("DELETE FROM AlbumArt WHERE id=?", [id])?;
        Ok(())
    }
}
