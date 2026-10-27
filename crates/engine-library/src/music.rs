use crate::{Database, Error, Playlist, Result, Track, backup::check, model::DurationUnit};
use audio_metadata::{
    inspect::{Inspection, inspect},
    process::Tools,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Default)]
pub struct ImportOptions {
    pub playlist: Option<String>,
    pub move_file: bool,
    pub duration_unit: DurationUnit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub track: Track,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<Playlist>,
    pub moved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_warning: Option<String>,
}

pub async fn import(
    database: &Database,
    library: &Path,
    file: &Path,
    options: &ImportOptions,
    tools: &Tools,
    cancel: &CancellationToken,
) -> Result<ImportResult> {
    check(cancel)?;
    let library = library.canonicalize()?;
    let source = file.canonicalize()?;
    let metadata = fs::metadata(&source)?;
    if !metadata.is_file() {
        return Err(Error::Invalid("expected an audio file".into()));
    }
    text(&source)?;
    let (inspection, probe_warning) = probe(tools, &source, cancel).await?;
    let target = if options.move_file {
        let directory = library.join("Music");
        fs::create_dir_all(&directory)?;
        directory.join(source.file_name().unwrap())
    } else {
        source.clone()
    };
    let path = stored_path(&library, &target)?;
    let movement = FileMove::prepare(&source, &target, cancel)?;
    let outcome = database.transaction(|database| {
        check(cancel)?;
        let existing = database
            .tracks()?
            .into_iter()
            .find(|track| track.path.as_deref() == Some(&path));
        let mut track = existing.clone().unwrap_or_default();
        track.path = Some(path);
        track.filename = Some(text(target.file_name().map(Path::new).unwrap())?.into());
        track.file_type = target
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase());
        track.file_bytes = Some(byte_count(metadata.len())?);
        track.is_available = Some(true);
        if existing.is_none() {
            track.is_analyzed = Some(false);
        }
        if let Some(inspection) = &inspection {
            merge(&mut track, inspection, options.duration_unit);
        }
        if track.title.as_deref().is_none_or(str::is_empty) {
            track.title = Some(text(target.file_stem().map(Path::new).unwrap())?.into());
        }
        if existing.is_some() {
            database.update_track(&track)?;
        } else {
            database.insert_track(&mut track)?;
        }
        let playlist = if let Some(identifier) = options
            .playlist
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let playlist = database
                .find_playlist(identifier)?
                .map(Ok)
                .unwrap_or_else(|| database.create_playlist(identifier, 0, false))?;
            database.add_membership_inner(playlist.id, track.id)?;
            Some(playlist)
        } else {
            None
        };
        Ok(ImportResult {
            track,
            playlist,
            moved: movement.changed,
            probe_warning,
        })
    });
    match outcome {
        Ok(result) => {
            movement.commit()?;
            Ok(result)
        }
        Err(error) => Err(movement.rollback(error)),
    }
}

pub fn move_file(
    database: &Database,
    library: &Path,
    identifier: &str,
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<Track> {
    check(cancel)?;
    if destination.as_os_str().is_empty() {
        return Err(Error::Invalid("destination path is empty".into()));
    }
    let library = library.canonicalize()?;
    let mut track = database
        .find_track(identifier)?
        .ok_or_else(|| Error::NotFound(format!("track {identifier:?}")))?;
    let source = library
        .join(
            track
                .path
                .as_deref()
                .ok_or_else(|| Error::Invalid("track has no file path".into()))?,
        )
        .canonicalize()?;
    let mut target = destination.to_path_buf();
    if destination.is_dir() || destination.as_os_str().as_encoded_bytes().ends_with(b"/") {
        target.push(source.file_name().unwrap());
    }
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let target = parent.canonicalize()?.join(
        target
            .file_name()
            .ok_or_else(|| Error::Invalid("destination has no filename".into()))?,
    );
    let stored = stored_path(&library, &target)?;
    let length = byte_count(fs::metadata(&source)?.len())?;
    let movement = FileMove::prepare(&source, &target, cancel)?;
    track.path = Some(stored);
    track.filename = Some(text(target.file_name().map(Path::new).unwrap())?.into());
    track.file_bytes = Some(length);
    match check(cancel).and_then(|()| database.update_track(&track)) {
        Ok(()) => {
            movement.commit()?;
            Ok(track)
        }
        Err(error) => Err(movement.rollback(error)),
    }
}

/// Commit database deletion first. Unlink failures retain audio and report a committed outcome.
pub fn remove(
    database: &Database,
    library: &Path,
    identifier: &str,
    delete_file: bool,
    cancel: &CancellationToken,
) -> Result<Track> {
    check(cancel)?;
    let track = database
        .find_track(identifier)?
        .ok_or_else(|| Error::NotFound(format!("track {identifier:?}")))?;
    database.delete_track(track.id)?;
    if delete_file && let Some(path) = track.path.as_deref().filter(|path| !path.is_empty()) {
        let path = library.join(path);
        match fs::remove_file(&path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(source) => return Err(Error::CommittedCleanup { path, source }),
        }
    }
    Ok(track)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub total_db_tracks: usize,
    pub updated_count: usize,
    pub missing_count: usize,
    pub unindexed_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub unindexed_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

pub async fn synchronize(
    database: &Database,
    library: &Path,
    unit: DurationUnit,
    tools: &Tools,
    cancel: &CancellationToken,
) -> Result<SyncReport> {
    check(cancel)?;
    let library = library.canonicalize()?;
    let tracks = database.tracks()?;
    let mut report = SyncReport {
        total_db_tracks: tracks.len(),
        ..Default::default()
    };
    let mut indexed = BTreeSet::new();
    for mut track in tracks {
        check(cancel)?;
        let Some(path) = track.path.as_deref().filter(|path| !path.is_empty()) else {
            report.missing_count += 1;
            continue;
        };
        let path = library.join(path);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                report.missing_count += 1;
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                report.missing_count += 1;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        indexed.insert(path.canonicalize()?);
        let previous = track.clone();
        let (inspection, warning) = probe(tools, &path, cancel).await?;
        if let Some(warning) = warning {
            report
                .warnings
                .push(format!("{}: {warning}", path.display()));
        }
        if let Some(inspection) = inspection {
            merge(&mut track, &inspection, unit);
        }
        track.file_bytes = Some(byte_count(metadata.len())?);
        if track != previous {
            database.update_track(&track)?;
            report.updated_count += 1;
        }
    }
    for entry in walkdir::WalkDir::new(&library)
        .follow_links(false)
        .sort_by_file_name()
    {
        check(cancel)?;
        let entry = entry.map_err(|error| {
            Error::Io(
                error
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("filesystem traversal failed")),
            )
        })?;
        if !entry.file_type().is_file() || !is_audio(entry.path()) {
            continue;
        }
        if !indexed.contains(&entry.path().canonicalize()?) {
            report
                .unindexed_paths
                .push(text(entry.path().strip_prefix(&library).unwrap())?.into());
        }
    }
    report.unindexed_count = report.unindexed_paths.len();
    Ok(report)
}

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["m4a", "mp3", "opus", "flac", "wav"]
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
}

async fn probe(
    tools: &Tools,
    path: &Path,
    cancel: &CancellationToken,
) -> Result<(Option<Inspection>, Option<String>)> {
    match inspect(tools, path, cancel).await {
        Ok(inspection) => Ok((Some(inspection), None)),
        Err(audio_metadata::Error::Cancelled) => Err(Error::Cancelled),
        Err(error) => Ok((None, Some(error.to_string()))),
    }
}

fn merge(track: &mut Track, inspection: &Inspection, unit: DurationUnit) {
    for (target, source) in [
        (&mut track.title, &inspection.title),
        (&mut track.artist, &inspection.artist),
        (&mut track.album, &inspection.album),
        (&mut track.genre, &inspection.genre),
        (&mut track.comment, &inspection.comment),
        (&mut track.label, &inspection.label),
        (&mut track.composer, &inspection.composer),
        (&mut track.remixer, &inspection.remixer),
    ] {
        if !source.is_empty() {
            *target = Some(source.clone());
        }
    }
    if let Some(year) = inspection.year.filter(|year| *year > 0) {
        track.year = Some(year.into());
    }
    if let Some(bpm) = inspection.bpm {
        track.bpm = Some(bpm as i64);
        track.bpm_analyzed = Some(bpm);
    }
    if let Some(length) = unit.from_seconds(inspection.duration_seconds) {
        track.length = Some(length);
    }
    if inspection.bit_rate_kbps > 0 {
        track.bitrate = Some(inspection.bit_rate_kbps.into());
    }
}

pub(crate) fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::Invalid(format!("SQLite path is not UTF-8: {}", path.display())))
}
fn stored_path(library: &Path, path: &Path) -> Result<String> {
    Ok(text(path.strip_prefix(library).unwrap_or(path))?.into())
}
fn byte_count(length: u64) -> Result<i64> {
    length
        .try_into()
        .map_err(|_| Error::Invalid("file size exceeds SQLite integer range".into()))
}

struct FileMove {
    source: PathBuf,
    target: PathBuf,
    changed: bool,
}
impl FileMove {
    fn prepare(source: &Path, target: &Path, cancel: &CancellationToken) -> Result<Self> {
        let changed = source != target;
        if changed {
            let mut temporary = tempfile::NamedTempFile::new_in(target.parent().unwrap())?;
            let mut input = fs::File::open(source)?;
            use std::io::{Read, Write};
            let mut buffer = [0; 65536];
            loop {
                check(cancel)?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                temporary.write_all(&buffer[..count])?;
            }
            temporary
                .as_file()
                .set_permissions(input.metadata()?.permissions())?;
            temporary.as_file().sync_all()?;
            temporary
                .persist_noclobber(target)
                .map_err(|error| Error::Io(error.error))?;
        }
        Ok(Self {
            source: source.into(),
            target: target.into(),
            changed,
        })
    }
    fn commit(self) -> Result<()> {
        if self.changed {
            fs::remove_file(&self.source).map_err(|source| Error::CommittedCleanup {
                path: self.source,
                source,
            })?;
        }
        Ok(())
    }
    fn rollback(self, primary: Error) -> Error {
        if self.changed
            && let Err(source) = fs::remove_file(&self.target)
        {
            return Error::Cleanup {
                primary: Box::new(primary),
                path: self.target,
                source,
            };
        }
        primary
    }
}

#[cfg(test)]
#[path = "../tests/unit/music.rs"]
mod tests;
