use crate::{Database, Error, Result, Track, backup::check, model::DurationUnit};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistNode {
    pub id: i64,
    pub title: String,
    pub parent_id: i64,
    pub track_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<PlaylistNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetails {
    pub id: i64,
    pub title: String,
    pub parent_id: i64,
    pub track_count: usize,
    pub total_duration_seconds: i64,
    pub tracks: Vec<Track>,
}

pub fn hierarchy(database: &Database) -> Result<Vec<PlaylistNode>> {
    let playlists = database.playlists()?;
    let by_id: BTreeMap<_, _> = playlists.iter().map(|list| (list.id, list)).collect();
    let mut depths = Vec::new();
    let mut nodes = BTreeMap::new();
    for list in &playlists {
        let mut ancestors = BTreeSet::new();
        let mut parent = list.parent_list_id;
        while parent != 0 {
            if parent == list.id || !ancestors.insert(parent) {
                return Err(Error::Integrity("cyclic playlist hierarchy".into()));
            }
            parent = by_id
                .get(&parent)
                .ok_or_else(|| {
                    Error::Integrity(format!("playlist {} has absent parent {parent}", list.id))
                })?
                .parent_list_id;
        }
        depths.push((ancestors.len(), list.id));
        nodes.insert(
            list.id,
            PlaylistNode {
                id: list.id,
                title: list.title.clone(),
                parent_id: list.parent_list_id,
                track_count: database.playlist_tracks(list.id)?.len(),
                children: Vec::new(),
            },
        );
    }
    depths.sort_by(|a, b| b.cmp(a));
    let mut roots = Vec::new();
    for (_, id) in depths {
        let mut node = nodes.remove(&id).unwrap();
        let ranks: BTreeMap<_, _> = database
            .playlist_siblings(id)?
            .iter()
            .enumerate()
            .map(|(rank, p)| (p.id, rank))
            .collect();
        node.children.sort_by_key(|child| ranks[&child.id]);
        if node.parent_id == 0 {
            roots.push(node);
        } else {
            nodes.get_mut(&node.parent_id).unwrap().children.push(node);
        }
    }
    let ranks: BTreeMap<_, _> = database
        .playlist_siblings(0)?
        .iter()
        .enumerate()
        .map(|(rank, p)| (p.id, rank))
        .collect();
    roots.sort_by_key(|node| ranks[&node.id]);
    Ok(roots)
}

pub fn inspect_playlist(
    database: &Database,
    identifier: &str,
    unit: DurationUnit,
) -> Result<PlaylistDetails> {
    let list = database
        .find_playlist(identifier)?
        .ok_or_else(|| Error::NotFound(format!("playlist {identifier:?}")))?;
    let tracks = database.playlist_tracks(list.id)?;
    let total = tracks
        .iter()
        .filter_map(|track| track.length)
        .try_fold(0i64, |sum, length| {
            sum.checked_add(length)
                .ok_or_else(|| Error::Integrity("playlist duration overflow".into()))
        })?;
    Ok(PlaylistDetails {
        id: list.id,
        title: list.title,
        parent_id: list.parent_list_id,
        track_count: tracks.len(),
        total_duration_seconds: unit.seconds(total),
        tracks,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    MissingOnDisk,
    FileBytesMismatch,
    MissingAlbumArt,
    OrphanedEntity,
    EmptyPlaylist,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    #[serde(rename = "type")]
    pub kind: IssueType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub total_tracks: usize,
    pub total_playlists: usize,
    pub missing_on_disk_count: usize,
    pub bytes_mismatch_count: usize,
    pub missing_art_count: usize,
    pub orphaned_count: usize,
    pub empty_playlist_count: usize,
    pub issues: Vec<Issue>,
    pub is_healthy: bool,
}

pub fn verify(
    database: &Database,
    music_base: &Path,
    cancel: &CancellationToken,
) -> Result<VerifyReport> {
    check(cancel)?;
    if music_base.as_os_str().is_empty() {
        return Err(Error::Invalid("music base directory is empty".into()));
    }
    let tracks = database.tracks()?;
    let playlists = database.playlists()?;
    let artwork: BTreeSet<_> = database.artwork()?.into_iter().map(|art| art.id).collect();
    let ids: BTreeSet<_> = tracks.iter().map(|track| track.id).collect();
    let lists: BTreeSet<_> = playlists.iter().map(|list| list.id).collect();
    let mut report = VerifyReport {
        total_tracks: tracks.len(),
        total_playlists: playlists.len(),
        missing_on_disk_count: 0,
        bytes_mismatch_count: 0,
        missing_art_count: 0,
        orphaned_count: 0,
        empty_playlist_count: 0,
        issues: Vec::new(),
        is_healthy: false,
    };
    for track in tracks {
        check(cancel)?;
        let path = track.path.as_deref().unwrap_or("");
        let full_path = music_base.join(path);
        match std::fs::metadata(&full_path) {
            Ok(metadata) if metadata.is_file() => {
                if track
                    .file_bytes
                    .is_some_and(|bytes| bytes > 0 && bytes as u64 != metadata.len())
                {
                    report.bytes_mismatch_count += 1;
                    report.issues.push(Issue {
                        kind: IssueType::FileBytesMismatch,
                        track_id: Some(track.id),
                        playlist_id: None,
                        path: Some(path.into()),
                        description: format!(
                            "Track {} fileBytes mismatch (DB: {}, Disk: {} bytes)",
                            track.id,
                            track.file_bytes.unwrap(),
                            metadata.len()
                        ),
                    });
                }
            }
            Ok(_) => missing(&mut report, &track, &full_path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing(&mut report, &track, &full_path)
            }
            Err(error) => return Err(error.into()),
        }
        if let Some(art) = track
            .album_art_id
            .filter(|id| *id > 0 && !artwork.contains(id))
        {
            report.missing_art_count += 1;
            report.issues.push(Issue {
                kind: IssueType::MissingAlbumArt,
                track_id: Some(track.id),
                playlist_id: None,
                path: None,
                description: format!("Track {} references missing AlbumArt {art}", track.id),
            });
        }
    }
    let mut statement = database
        .connection()
        .prepare("SELECT id,listId,trackId FROM PlaylistEntity ORDER BY id")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })? {
        check(cancel)?;
        let (entity, list, track) = row?;
        if !ids.contains(&track) || !lists.contains(&list) {
            report.orphaned_count += 1;
            report.issues.push(Issue {
                kind: IssueType::OrphanedEntity,
                track_id: Some(track),
                playlist_id: Some(list),
                path: None,
                description: format!(
                    "PlaylistEntity {entity} references absent track {track} or playlist {list}"
                ),
            });
        }
    }
    for list in playlists {
        check(cancel)?;
        if list.is_folder != Some(true) && database.playlist_tracks(list.id)?.is_empty() {
            report.empty_playlist_count += 1;
            report.issues.push(Issue {
                kind: IssueType::EmptyPlaylist,
                track_id: None,
                playlist_id: Some(list.id),
                path: None,
                description: format!(
                    "Playlist {:?} (ID {}) contains 0 tracks",
                    list.title, list.id
                ),
            });
        }
    }
    report.is_healthy = report.issues.is_empty();
    Ok(report)
}

fn missing(report: &mut VerifyReport, track: &Track, full_path: &Path) {
    report.missing_on_disk_count += 1;
    report.issues.push(Issue {
        kind: IssueType::MissingOnDisk,
        track_id: Some(track.id),
        playlist_id: None,
        path: track.path.clone(),
        description: format!(
            "Track {} not found on disk at {}",
            track.id,
            full_path.display()
        ),
    });
}
