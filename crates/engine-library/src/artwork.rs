//! Native embedded-cover decoding, cache generation, and reference-aware pruning.
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use image::{DynamicImage, GenericImageView, ImageReader, imageops::FilterType};
use lofty::{file::TaggedFileExt, picture::PictureType};
use serde::Serialize;
use sha1::{Digest, Sha1};
use tokio_util::sync::CancellationToken;

use crate::{Database, Error, Result, Track, backup::check as check_cancel};

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeReport {
    pub total_tracks: usize,
    pub normalized_count: usize,
    pub already_square: usize,
    pub no_art_count: usize,
    pub error_count: usize,
    pub cache_normalized_count: usize,
    pub warnings: Vec<String>,
}

impl NormalizeReport {
    fn failure(&mut self, context: impl std::fmt::Display, error: Error) -> Result<()> {
        if matches!(error, Error::Cancelled) {
            return Err(error);
        }
        self.error_count += 1;
        self.warnings.push(format!("{context}: {error}"));
        Ok(())
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneReport {
    pub deleted_db_rows: usize,
    pub deleted_disk_files: usize,
}

/// Update collection artwork without modifying the audio files or their tags.
/// Each database image/track update is transactional. Cache images are published first;
/// a failed database update can leave an unreferenced cache image removable by `prune`.
pub fn normalize(
    db: &Database,
    music: &Path,
    cache: &Path,
    cancel: &CancellationToken,
) -> Result<NormalizeReport> {
    check_cancel(cancel)?;
    if music.as_os_str().is_empty() {
        return Err(Error::Invalid("empty music base directory".into()));
    }
    if !cache.as_os_str().is_empty() {
        std::fs::create_dir_all(cache)?;
    }
    let tracks = db.tracks()?;
    let mut report = NormalizeReport {
        total_tracks: tracks.len(),
        ..Default::default()
    };
    for mut track in tracks {
        check_cancel(cancel)?;
        let Some(path) = track.path.as_deref().filter(|path| !path.is_empty()) else {
            report.no_art_count += 1;
            continue;
        };
        let result = embedded(&music.join(path)).and_then(|bytes| {
            let Some(bytes) = bytes else {
                return Ok(None);
            };
            let image = decode(&bytes)?;
            let dimensions = image.dimensions();
            let prepared = Prepared::new(&image)?;
            prepared.publish(cache, cancel)?;
            db.transaction(|db| {
                let id = db.upsert_art(&prepared.hash, &prepared.thumbnail)?;
                set_track_art(&mut track, id, &prepared.hash);
                db.update_track(&track)
            })?;
            Ok(Some(dimensions))
        });
        match result {
            Ok(None) => report.no_art_count += 1,
            Ok(Some((width, height))) if width == height && width >= 1000 => {
                report.already_square += 1
            }
            Ok(Some(_)) => report.normalized_count += 1,
            Err(error) => report.failure(format!("track {}", track.id), error)?,
        }
    }
    // Normalize old database blobs as well as the covers embedded in audio files.
    for art in db.artwork()? {
        check_cancel(cancel)?;
        if art.data.is_empty() {
            continue;
        }
        let result = (|| {
            let image = decode(&art.data)?;
            if image.width() == image.height() {
                return Ok(false);
            }
            let prepared = Prepared::new(&image)?;
            prepared.publish(cache, cancel)?;
            db.transaction(|db| {
                db.update_art(art.id, &prepared.hash, &prepared.thumbnail)?;
                for mut track in db
                    .tracks()?
                    .into_iter()
                    .filter(|track| track.album_art_id == Some(art.id))
                {
                    set_track_art(&mut track, art.id, &prepared.hash);
                    db.update_track(&track)?;
                }
                Ok(())
            })?;
            Ok(true)
        })();
        match result {
            Ok(true) => report.normalized_count += 1,
            Ok(false) => {}
            Err(error) => report.failure(format!("artwork {}", art.id), error)?,
        }
    }
    let references = cache_references(db)?;
    for path in cache_files(cache)? {
        check_cancel(cancel)?;
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jpg"))
        {
            continue;
        }
        let result = (|| {
            let image = decode(&std::fs::read(&path)?)?;
            if image.width() == image.height() {
                return Ok(false);
            }
            let prepared = Prepared::new(&image)?;
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let referenced = references.get(name);
            let hashed = hex::decode(name)
                .ok()
                .is_some_and(|bytes| bytes.len() == 20)
                || URL_SAFE_NO_PAD
                    .decode(name)
                    .ok()
                    .is_some_and(|bytes| bytes.len() == 20);
            if referenced.is_some() || hashed {
                prepared.publish(cache, cancel)?;
                if let Some(referenced) = referenced {
                    db.transaction(|db| {
                        for id in &referenced.art {
                            db.update_art(*id, &prepared.hash, &prepared.thumbnail)?;
                        }
                        for id in &referenced.tracks {
                            let mut track = db
                                .track(*id)?
                                .ok_or_else(|| Error::NotFound(format!("track {id}")))?;
                            track.album_art_source_hash = Some(prepared.hash.clone());
                            db.update_track(&track)?;
                        }
                        Ok(())
                    })?;
                }
                if name != prepared.hash && name != prepared.base64 {
                    std::fs::remove_file(&path).map_err(|source| Error::CommittedCleanup {
                        path: path.clone(),
                        source,
                    })?;
                }
            } else {
                atomic_write(&path, &prepared.full, cancel)?;
            }
            Ok(true)
        })();
        match result {
            Ok(true) => report.cache_normalized_count += 1,
            Ok(false) => {}
            Err(error) => report.failure(path.display(), error)?,
        }
    }
    Ok(report)
}

#[derive(Default)]
struct References {
    art: BTreeSet<i64>,
    tracks: BTreeSet<i64>,
}

fn cache_references(db: &Database) -> Result<BTreeMap<String, References>> {
    let mut references: BTreeMap<String, References> = BTreeMap::new();
    let tracks = db.tracks()?;
    let mut by_art: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for track in &tracks {
        if let Some(art) = track.album_art_id {
            by_art.entry(art).or_default().push(track.id);
        }
    }
    for art in db.artwork()? {
        let Some(hash) = art.hash else {
            continue;
        };
        let mut names = BTreeSet::new();
        reference_names(&hash, &mut names);
        for name in names {
            let entry = references.entry(name).or_default();
            entry.art.insert(art.id);
            if let Some(ids) = by_art.get(&art.id) {
                entry.tracks.extend(ids);
            }
        }
    }
    for track in tracks {
        let Some(hash) = track.album_art_source_hash else {
            continue;
        };
        let mut names = BTreeSet::new();
        reference_names(&hash, &mut names);
        for name in names {
            references.entry(name).or_default().tracks.insert(track.id);
        }
    }
    Ok(references)
}

fn embedded(path: &Path) -> Result<Option<Vec<u8>>> {
    // Opening first keeps native filesystem errors distinct from malformed metadata.
    std::fs::File::open(path)?;
    let tagged = lofty::read_from_path(path)
        .map_err(|error| Error::Invalid(format!("invalid audio tags: {error}")))?;
    let pictures: Vec<_> = tagged
        .tags()
        .iter()
        .flat_map(|tag| tag.pictures())
        .collect();
    let picture = pictures
        .iter()
        .find(|picture| picture.pic_type() == PictureType::CoverFront)
        .copied()
        .or_else(|| pictures.first().copied());
    picture
        .map(|picture| {
            if picture.data().len() > 20 * 1024 * 1024 {
                return Err(Error::Invalid("embedded artwork exceeds 20 MiB".into()));
            }
            Ok(picture.data().to_vec())
        })
        .transpose()
}

fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?)
}

struct Prepared {
    full: Vec<u8>,
    thumbnail: Vec<u8>,
    hash: String,
    base64: String,
}
impl Prepared {
    fn new(image: &DynamicImage) -> Result<Self> {
        let full = jpeg(&image.resize_to_fill(1400, 1400, FilterType::Lanczos3))?;
        let thumbnail = jpeg(&image.resize_to_fill(256, 256, FilterType::Lanczos3))?;
        let digest = Sha1::digest(&full);
        Ok(Self {
            full,
            thumbnail,
            hash: hex::encode(digest),
            base64: URL_SAFE_NO_PAD.encode(digest),
        })
    }
    fn publish(&self, cache: &Path, cancel: &CancellationToken) -> Result<()> {
        if !cache.as_os_str().is_empty() {
            atomic_write(
                &cache.join(format!("{}.jpg", self.hash)),
                &self.full,
                cancel,
            )?;
            atomic_write(
                &cache.join(format!("{}.jpg", self.base64)),
                &self.full,
                cancel,
            )?;
        }
        Ok(())
    }
}
fn jpeg(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
        .encode_image(&image.to_rgb8())?;
    Ok(bytes)
}
fn atomic_write(path: &Path, bytes: &[u8], cancel: &CancellationToken) -> Result<()> {
    let mut staged = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| Error::Invalid("cache file has no parent".into()))?,
    )?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    check_cancel(cancel)?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}
fn set_track_art(track: &mut Track, id: i64, hash: &str) {
    track.album_art_id = Some(id);
    track.album_art = Some(format!("image://planck/{id}"));
    track.album_art_source_hash = Some(hash.into());
}

fn cache_files(cache: &Path) -> Result<Vec<PathBuf>> {
    if cache.as_os_str().is_empty() {
        return Ok(Vec::new());
    }
    let entries = match std::fs::read_dir(cache) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Delete unreferenced database images and regular cache files, preserving exact base64 case.
pub fn prune(db: &Database, cache: &Path, cancel: &CancellationToken) -> Result<PruneReport> {
    check_cancel(cancel)?;
    let tracks = db.tracks()?;
    let referenced: BTreeSet<_> = tracks
        .iter()
        .filter_map(|track| track.album_art_id)
        .collect();
    let mut names = BTreeSet::new();
    for hash in tracks
        .iter()
        .filter_map(|track| track.album_art_source_hash.as_deref())
    {
        reference_names(hash, &mut names);
    }
    let mut deleting = Vec::new();
    for art in db.artwork()? {
        if referenced.contains(&art.id) {
            if let Some(hash) = art.hash {
                reference_names(&hash, &mut names);
            }
        } else {
            deleting.push(art.id);
        }
    }
    let files = cache_files(cache)?;
    db.transaction(|db| {
        for id in &deleting {
            check_cancel(cancel)?;
            db.delete_art(*id)?;
        }
        Ok(())
    })?;
    let mut report = PruneReport {
        deleted_db_rows: deleting.len(),
        deleted_disk_files: 0,
    };
    for path in files {
        check_cancel(cancel)?;
        if path
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| names.contains(name))
        {
            continue;
        }
        std::fs::remove_file(&path).map_err(|source| Error::CommittedCleanup {
            path: path.clone(),
            source,
        })?;
        report.deleted_disk_files += 1;
    }
    Ok(report)
}
fn reference_names(hash: &str, names: &mut BTreeSet<String>) {
    if hash.is_empty() {
        return;
    }
    names.insert(hash.to_owned());
    if let Ok(bytes) = hex::decode(hash) {
        names.insert(hex::encode(&bytes));
        names.insert(URL_SAFE_NO_PAD.encode(bytes));
    }
}
