//! SQLite-aware snapshots. Each database is consistent; separate databases are not one transaction.
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub db_files: usize,
    pub art_files: usize,
    pub size_bytes: u64,
}

pub fn default_directory() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|p| p.join("engine-cli/backups"))
        .ok_or_else(|| Error::Invalid("cannot resolve user data directory".into()))
}

pub fn create(
    library: &Path,
    output: Option<&Path>,
    cancel: &CancellationToken,
) -> Result<BackupInfo> {
    check(cancel)?;
    require_directory(library)?;
    let output = output
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(default_directory)?;
    fs::create_dir_all(&output)?;
    let stage = tempfile::Builder::new()
        .prefix(".snapshot-")
        .tempdir_in(&output)?;
    for (source, target) in [
        (library.join("Database2"), "Database2"),
        (library.to_path_buf(), "Root"),
    ] {
        let destination = stage.path().join(target);
        fs::create_dir(&destination)?;
        for source in files(&source, true)? {
            check(cancel)?;
            let name = source
                .file_name()
                .ok_or_else(|| Error::Invalid("database has no filename".into()))?;
            copy_database(&source, &destination.join(name), cancel)?;
        }
    }
    let art = files(&library.join("Artwork"), false)?;
    if !art.is_empty() {
        fs::create_dir(stage.path().join("Artwork"))?;
        for source in art {
            let name = source
                .file_name()
                .ok_or_else(|| Error::Invalid("artwork has no filename".into()))?;
            copy_file(&source, &stage.path().join("Artwork").join(name), cancel)?;
        }
    }
    check(cancel)?;
    // tempfile's random suffix prevents same-clock collisions without replacing an existing snapshot.
    let suffix = stage.path().file_name().unwrap().to_string_lossy();
    let name = format!(
        "backup_{}_{}",
        Utc::now().format("%Y%m%d_%H%M%S%9f"),
        &suffix[10..]
    );
    let destination = output.join(name);
    fs::rename(stage.path(), &destination)?;
    let _retained_path = stage.keep();
    inspect(&destination)
}

pub fn automatic(
    library: &Path,
    disabled: bool,
    cancel: &CancellationToken,
) -> Result<Option<BackupInfo>> {
    if disabled {
        Ok(None)
    } else {
        create(library, None, cancel).map(Some)
    }
}

pub fn list(directory: &Path) -> Result<Vec<BackupInfo>> {
    let mut result = Vec::new();
    for entry in optional_entries(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.file_name().to_string_lossy().starts_with("backup_")
        {
            result.push(inspect(&entry.path())?);
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

/// Preflight every source before writing. A later failure reports the exact completed paths.
/// SQLite restores are atomic per database, including when the destination already uses WAL.
pub fn restore(snapshot: &Path, library: &Path, cancel: &CancellationToken) -> Result<()> {
    check(cancel)?;
    require_directory(snapshot)?;
    require_directory(library)?;
    let mut databases = Vec::new();
    for (source, destination) in [
        (snapshot.join("Database2"), library.join("Database2")),
        (snapshot.join("Root"), library.to_path_buf()),
    ] {
        for source in files(&source, true)? {
            check(cancel)?;
            let connection = read_database(&source)?;
            let health: String =
                connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if health != "ok" {
                return Err(Error::Integrity(format!("{}: {health}", source.display())));
            }
            let name = source
                .file_name()
                .ok_or_else(|| Error::Invalid("database has no filename".into()))?;
            databases.push((source.clone(), destination.join(name)));
        }
    }
    if databases.is_empty() {
        return Err(Error::Invalid("snapshot contains no databases".into()));
    }
    let artwork = files(&snapshot.join("Artwork"), false)?;
    // Opening all files before the first write catches unreadable artwork during preflight too.
    for source in &artwork {
        fs::File::open(source)?;
    }
    let mut completed = Vec::new();
    let outcome = (|| {
        for (source, destination) in databases {
            fs::create_dir_all(destination.parent().unwrap())?;
            copy_database(&source, &destination, cancel)?;
            completed.push(destination);
        }
        if !artwork.is_empty() {
            fs::create_dir_all(library.join("Artwork"))?;
        }
        for source in artwork {
            let name = source.file_name().unwrap();
            let destination = library.join("Artwork").join(name);
            let mut temporary = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
            copy_to(&source, temporary.as_file_mut(), cancel)?;
            temporary.as_file().sync_all()?;
            temporary
                .persist(&destination)
                .map_err(|error| Error::Io(error.error))?;
            completed.push(destination);
        }
        Ok(())
    })();
    match outcome {
        Err(source) if !completed.is_empty() => Err(Error::PartialRestore {
            completed,
            source: Box::new(source),
        }),
        other => other,
    }
}

fn read_database(path: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

fn copy_database(source: &Path, destination: &Path, cancel: &CancellationToken) -> Result<()> {
    let source = read_database(source)?;
    let mut destination = Connection::open(destination)?;
    let backup = Backup::new(&source, &mut destination)?;
    let mut last_progress = Instant::now();
    loop {
        check(cancel)?;
        match backup.step(128)? {
            StepResult::Done => return Ok(()),
            StepResult::More => last_progress = Instant::now(),
            StepResult::Busy | StepResult::Locked => {
                if last_progress.elapsed() > Duration::from_secs(5) {
                    return Err(Error::Database(rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                        Some("backup remained busy for five seconds".into()),
                    )));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => return Err(Error::Integrity("unknown SQLite backup state".into())),
        }
    }
}

fn copy_file(source: &Path, destination: &Path, cancel: &CancellationToken) -> Result<()> {
    let mut destination = fs::File::create(destination)?;
    copy_to(source, &mut destination, cancel)?;
    destination.sync_all()?;
    Ok(())
}

fn copy_to(source: &Path, destination: &mut fs::File, cancel: &CancellationToken) -> Result<()> {
    let mut source = fs::File::open(source)?;
    let mut buffer = [0; 65536];
    loop {
        check(cancel)?;
        let count = source.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        destination.write_all(&buffer[..count])?;
    }
}

fn optional_entries(path: &Path) -> Result<Vec<std::io::Result<fs::DirEntry>>> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn files(directory: &Path, databases_only: bool) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for entry in optional_entries(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if !databases_only
            || entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("db"))
        {
            result.push(entry.path());
        }
    }
    result.sort();
    Ok(result)
}

fn inspect(path: &Path) -> Result<BackupInfo> {
    let mut db_files = 0;
    let mut art_files = 0;
    let mut size_bytes = 0;
    for section in ["Database2", "Root", "Artwork"] {
        for file in files(&path.join(section), section != "Artwork")? {
            size_bytes += file.metadata()?.len();
            if section == "Artwork" {
                art_files += 1;
            } else {
                db_files += 1;
            }
        }
    }
    Ok(BackupInfo {
        path: path.into(),
        name: path.file_name().unwrap().to_string_lossy().into(),
        timestamp: path.metadata()?.modified()?.into(),
        db_files,
        art_files,
        size_bytes,
    })
}

fn require_directory(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Err(Error::Invalid(format!(
            "not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn check(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
