use super::{Database, Error, Playlist, Result, columns};
use std::collections::{BTreeMap, BTreeSet};

impl Database {
    /// Follow native sibling links; append disconnected legacy chains by ID.
    pub fn playlist_siblings(&self, parent: i64) -> Result<Vec<Playlist>> {
        let siblings: Vec<_> = self
            .playlists()?
            .into_iter()
            .filter(|p| p.parent_list_id == parent)
            .collect();
        if !columns(&self.connection, "Playlist")?.contains("nextListId") {
            return Ok(siblings);
        }
        let links:BTreeMap<i64,i64>=self.connection.prepare("SELECT id,COALESCE(nextListId,0) FROM Playlist WHERE COALESCE(parentListId,0)=? ORDER BY id")?.query_map([parent],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let referenced: BTreeSet<_> = links.values().copied().collect();
        let by_id: BTreeMap<_, _> = siblings.iter().map(|p| (p.id, p)).collect();
        let mut visited = BTreeSet::new();
        let mut ordered = Vec::new();
        for head in siblings
            .iter()
            .filter(|p| !referenced.contains(&p.id))
            .chain(siblings.iter())
        {
            let mut cursor = head.id;
            while cursor != 0 && visited.insert(cursor) {
                let Some(playlist) = by_id.get(&cursor) else {
                    break;
                };
                ordered.push((*playlist).clone());
                cursor = links[&cursor];
            }
        }
        Ok(ordered)
    }

    /// Replace the complete sibling order in one transaction.
    pub fn reorder_playlists(&self, parent: i64, ids: &[i64]) -> Result<()> {
        self.transaction(|database| database.reorder_playlists_inner(parent, ids))
    }

    pub(super) fn reorder_playlists_inner(&self, parent: i64, ids: &[i64]) -> Result<()> {
        let expected: BTreeSet<_> = self
            .playlists()?
            .iter()
            .filter(|p| p.parent_list_id == parent)
            .map(|p| p.id)
            .collect();
        let actual: BTreeSet<_> = ids.iter().copied().collect();
        if expected != actual || actual.len() != ids.len() {
            return Err(Error::Invalid(
                "order must contain every sibling exactly once".into(),
            ));
        }
        if !columns(&self.connection, "Playlist")?.contains("nextListId") {
            return Ok(());
        }
        // NULL is essential: a native UNIQUE(parentListId,nextListId) index permits
        // moving links without temporarily duplicating a successor or the tail.
        self.connection.execute(
            "UPDATE Playlist SET nextListId=NULL WHERE COALESCE(parentListId,0)=?",
            [parent],
        )?;
        for (index, id) in ids.iter().enumerate() {
            self.connection.execute(
                "UPDATE Playlist SET nextListId=? WHERE id=?",
                rusqlite::params![ids.get(index + 1).copied().unwrap_or(0), id],
            )?;
        }
        Ok(())
    }
}
