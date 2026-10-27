use super::{Database, Error, Playlist, PlaylistEntity, Result, Track, identifier, unique};
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};

impl Database {
    /// Insert a playlist with an explicit interchange identity, appending it to its siblings.
    /// Existing identities and invalid parents are rejected without altering the collection.
    pub fn insert_playlist_record(&self, playlist: &Playlist) -> Result<()> {
        if self.connection.is_autocommit() {
            return self.transaction(|database| database.insert_playlist_record_inner(playlist));
        }
        self.insert_playlist_record_inner(playlist)
    }

    fn insert_playlist_record_inner(&self, playlist: &Playlist) -> Result<()> {
        if playlist.id <= 0 {
            return Err(Error::Invalid("playlist ID must be positive".into()));
        }
        let title = identifier(&playlist.title)?;
        if playlist.parent_list_id != 0 {
            self.require_playlist(playlist.parent_list_id)?;
        }
        let mut order: Vec<_> = self
            .playlist_siblings(playlist.parent_list_id)?
            .iter()
            .map(|p| p.id)
            .collect();
        if self.folder_flag {
            self.connection.execute(
                "INSERT INTO Playlist(id,title,parentListId,isFolder) VALUES(?,?,?,?)",
                params![
                    playlist.id,
                    title,
                    playlist.parent_list_id,
                    playlist.is_folder
                ],
            )?;
        } else {
            if playlist.is_folder == Some(true) {
                return Err(Error::Schema("independent folder flag is absent".into()));
            }
            self.connection.execute(
                "INSERT INTO Playlist(id,title,parentListId) VALUES(?,?,?)",
                params![playlist.id, title, playlist.parent_list_id],
            )?;
        }
        order.push(playlist.id);
        self.reorder_playlists_inner(playlist.parent_list_id, &order)
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let flag = if self.folder_flag { "isFolder" } else { "NULL" };
        let mut statement = self.connection.prepare(&format!(
            "SELECT id,title,COALESCE(parentListId,0),{flag} FROM Playlist ORDER BY parentListId,id"
        ))?;
        Ok(statement
            .query_map([], |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    parent_list_id: row.get(2)?,
                    is_folder: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn find_playlist(&self, query: &str) -> Result<Option<Playlist>> {
        let query = identifier(query)?;
        let lists = self.playlists()?;
        if let Ok(id) = query.parse::<i64>()
            && let Some(playlist) = lists.iter().find(|playlist| playlist.id == id)
        {
            return Ok(Some(playlist.clone()));
        }
        unique(
            "playlist",
            query,
            lists
                .into_iter()
                .filter(|playlist| playlist.title.eq_ignore_ascii_case(query))
                .collect(),
            |playlist| playlist.id,
        )
    }

    pub fn require_playlist(&self, id: i64) -> Result<Playlist> {
        self.playlists()?
            .into_iter()
            .find(|playlist| playlist.id == id)
            .ok_or_else(|| Error::NotFound(format!("playlist {id}")))
    }

    pub fn create_playlist(&self, title: &str, parent: i64, folder: bool) -> Result<Playlist> {
        if self.connection.is_autocommit() {
            return self
                .transaction(|database| database.create_playlist_inner(title, parent, folder));
        }
        self.create_playlist_inner(title, parent, folder)
    }

    fn create_playlist_inner(&self, title: &str, parent: i64, folder: bool) -> Result<Playlist> {
        let title = identifier(title)?;
        if parent != 0 {
            self.require_playlist(parent)?;
        }
        let mut siblings: Vec<_> = self
            .playlist_siblings(parent)?
            .iter()
            .map(|p| p.id)
            .collect();
        if self.folder_flag {
            self.connection.execute(
                "INSERT INTO Playlist(title,parentListId,isFolder) VALUES (?,?,?)",
                params![title, parent, folder],
            )?;
        } else {
            if folder {
                return Err(Error::Schema(
                    "an independent folder flag is absent; create a playlist container instead"
                        .into(),
                ));
            }
            self.connection.execute(
                "INSERT INTO Playlist(title,parentListId) VALUES (?,?)",
                params![title, parent],
            )?;
        }
        let id = self.connection.last_insert_rowid();
        siblings.push(id);
        self.reorder_playlists_inner(parent, &siblings)?;
        self.require_playlist(id)
    }

    pub fn move_playlist(&self, id: i64, parent: i64) -> Result<()> {
        self.transaction(|database| {
            let current = database.require_playlist(id)?;
            if current.parent_list_id == parent {
                return Ok(());
            }
            let old_order: Vec<_> = database
                .playlist_siblings(current.parent_list_id)?
                .iter()
                .filter(|p| p.id != id)
                .map(|p| p.id)
                .collect();
            let mut new_order: Vec<_> = database
                .playlist_siblings(parent)?
                .iter()
                .map(|p| p.id)
                .collect();
            let mut ancestors = BTreeSet::new();
            let mut cursor = parent;
            while cursor != 0 {
                if cursor == id || !ancestors.insert(cursor) {
                    return Err(Error::Integrity("cyclic playlist parent chain".into()));
                }
                cursor = database.require_playlist(cursor)?.parent_list_id;
            }
            if super::columns(&database.connection, "Playlist")?.contains("nextListId") {
                database
                    .connection
                    .execute("UPDATE Playlist SET nextListId=NULL WHERE id=?", [id])?;
            }
            database.connection.execute(
                "UPDATE Playlist SET parentListId=? WHERE id=?",
                params![parent, id],
            )?;
            new_order.push(id);
            database.reorder_playlists_inner(current.parent_list_id, &old_order)?;
            database.reorder_playlists_inner(parent, &new_order)?;
            Ok(())
        })
    }

    pub fn delete_playlist(&self, id: i64, force: bool) -> Result<()> {
        self.transaction(|database| {
            let current = database.require_playlist(id)?;
            let surviving: Vec<_> = database
                .playlist_siblings(current.parent_list_id)?
                .iter()
                .filter(|p| p.id != id)
                .map(|p| p.id)
                .collect();
            let playlists = database.playlists()?;
            let mut pending = vec![id];
            let mut deleting = BTreeSet::new();
            let mut order = Vec::new();
            while let Some(parent) = pending.pop() {
                if !deleting.insert(parent) {
                    return Err(Error::Integrity("cyclic playlist hierarchy".into()));
                }
                order.push(parent);
                for child in playlists
                    .iter()
                    .filter(|playlist| playlist.parent_list_id == parent)
                {
                    if !force {
                        return Err(Error::Invalid(
                            "playlist has children; force is required".into(),
                        ));
                    }
                    pending.push(child.id);
                }
            }
            for id in order.into_iter().rev() {
                database
                    .connection
                    .execute("DELETE FROM PlaylistEntity WHERE listId=?", [id])?;
                database
                    .connection
                    .execute("DELETE FROM Playlist WHERE id=?", [id])?;
            }
            database.reorder_playlists_inner(current.parent_list_id, &surviving)?;
            Ok(())
        })
    }

    pub fn entities(&self, list: i64) -> Result<Vec<PlaylistEntity>> {
        let mut statement = self.connection.prepare("SELECT id,listId,trackId,COALESCE(nextEntityId,0),COALESCE(membershipReference,0) FROM PlaylistEntity WHERE listId=? ORDER BY id")?;
        Ok(statement
            .query_map([list], |row| {
                Ok(PlaylistEntity {
                    id: row.get(0)?,
                    list_id: row.get(1)?,
                    track_id: row.get(2)?,
                    next_entity_id: row.get(3)?,
                    membership_reference: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Traverse native nextEntityId ordering. Legacy disconnected chains are appended by ID.
    pub fn playlist_tracks(&self, list: i64) -> Result<Vec<Track>> {
        let entities = self.entities(list)?;
        let references: BTreeSet<_> = entities
            .iter()
            .map(|entity| entity.next_entity_id)
            .collect();
        let by_id: BTreeMap<_, _> = entities.iter().map(|entity| (entity.id, entity)).collect();
        let mut visited = BTreeSet::new();
        let mut tracks = Vec::new();
        let heads = entities
            .iter()
            .filter(|entity| !references.contains(&entity.id))
            .chain(entities.iter());
        for head in heads {
            let mut cursor = head.id;
            while cursor != 0 && visited.insert(cursor) {
                let Some(entity) = by_id.get(&cursor) else {
                    break;
                };
                if let Some(track) = self.track(entity.track_id)? {
                    tracks.push(track);
                }
                cursor = entity.next_entity_id;
            }
        }
        Ok(tracks)
    }

    pub fn add_membership(&self, list: i64, track: i64) -> Result<i64> {
        self.transaction(|database| database.add_membership_inner(list, track))
    }

    pub(crate) fn add_membership_inner(&self, list: i64, track: i64) -> Result<i64> {
        self.require_playlist(list)?;
        self.track(track)?
            .ok_or_else(|| Error::NotFound(format!("track {track}")))?;
        let entities = self.entities(list)?;
        if let Some(entity) = entities.iter().find(|entity| entity.track_id == track) {
            return Ok(entity.id);
        }
        let tail = entities
            .iter()
            .rev()
            .find(|entity| entity.next_entity_id == 0);
        if !entities.is_empty() && tail.is_none() {
            return Err(Error::Integrity("playlist has no tail".into()));
        }
        self.connection.execute("INSERT INTO PlaylistEntity(listId,trackId,nextEntityId,membershipReference) VALUES (?,?,0,0)", params![list,track])?;
        let id = self.connection.last_insert_rowid();
        if super::columns(&self.connection, "PlaylistEntity")?.contains("databaseUuid")
            && let Some(info) = self.information()?
        {
            self.connection.execute(
                "UPDATE PlaylistEntity SET databaseUuid=? WHERE id=?",
                params![info.uuid.to_string(), id],
            )?;
        }
        if let Some(tail) = tail {
            self.connection.execute(
                "UPDATE PlaylistEntity SET nextEntityId=? WHERE id=?",
                params![id, tail.id],
            )?;
        }
        Ok(id)
    }

    pub fn remove_membership(&self, list: i64, track: i64) -> Result<()> {
        self.transaction(|database| database.remove_membership_inner(list, track))
    }

    pub(super) fn remove_membership_inner(&self, list: i64, track: i64) -> Result<()> {
        let mut removed = false;
        loop {
            let entities = self.entities(list)?;
            let Some(entity) = entities.iter().find(|entity| entity.track_id == track) else {
                return if removed {
                    Ok(())
                } else {
                    Err(Error::NotFound(format!("track {track} in playlist {list}")))
                };
            };
            let next = if entity.next_entity_id == entity.id {
                0
            } else {
                entity.next_entity_id
            };
            self.connection.execute(
                "UPDATE PlaylistEntity SET nextEntityId=? WHERE listId=? AND nextEntityId=?",
                params![next, list, entity.id],
            )?;
            self.connection
                .execute("DELETE FROM PlaylistEntity WHERE id=?", [entity.id])?;
            removed = true;
        }
    }

    pub fn move_membership(&self, source: i64, destination: i64, track: i64) -> Result<()> {
        if source == destination {
            return Ok(());
        }
        self.transaction(|database| {
            database.remove_membership_inner(source, track)?;
            database.add_membership_inner(destination, track)?;
            Ok(())
        })
    }
}
