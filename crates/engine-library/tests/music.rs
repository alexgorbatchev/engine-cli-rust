mod support;
use audio_metadata::process::Tools;
use engine_library::{
    Database, Error, Track,
    model::DurationUnit,
    music::{ImportOptions, import, move_file, remove, synchronize},
};
use rusqlite::Connection;
use std::{path::Path, process::Command};
use tokio_util::sync::CancellationToken;

fn database() -> Database {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    Database::from_connection(connection).unwrap()
}
fn audio(path: &Path) {
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.4",
            "-metadata",
            "title=Native title",
            "-metadata",
            "artist=Artist",
            "-metadata",
            "album=Album",
            "-metadata",
            "date=2026-01-02",
            "-metadata",
            "BPM=123.5",
            "-metadata",
            "comment=Notes",
            "-metadata",
            "label=Label",
            "-metadata",
            "composer=Composer",
            "-metadata",
            "remixer=Remixer",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn tools(root: &Path) -> Tools {
    Tools::new(std::env::var_os("PATH").unwrap(), root.into())
}

#[tokio::test]
async fn native_audio_import_sync_and_membership_preserve_existing_metadata() {
    let root = support::scratch();
    let file = root.path().join("tone.flac");
    audio(&file);
    let db = database();
    let cancel = CancellationToken::new();
    let tools = tools(root.path());
    let options = ImportOptions {
        playlist: Some("set".into()),
        ..Default::default()
    };
    let added = import(&db, root.path(), &file, &options, &tools, &cancel)
        .await
        .unwrap();
    assert_eq!(added.track.title.as_deref(), Some("Native title"));
    assert_eq!(added.track.length, Some(400));
    assert_eq!(added.track.bpm_analyzed, Some(123.5));
    assert_eq!(added.track.year, Some(2026));
    assert_eq!(added.track.composer.as_deref(), Some("Composer"));
    assert!(added.probe_warning.is_none());
    assert_eq!(
        db.playlist_tracks(added.playlist.unwrap().id)
            .unwrap()
            .len(),
        1
    );
    let mut existing = added.track;
    existing.rating = Some(5);
    existing.album_art_id = Some(123);
    db.update_track(&existing).unwrap();
    let again = import(
        &db,
        root.path(),
        &file,
        &ImportOptions::default(),
        &tools,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(
        (again.track.id, again.track.rating, again.track.album_art_id),
        (existing.id, Some(5), Some(123))
    );
    existing.file_bytes = Some(1);
    db.update_track(&existing).unwrap();
    std::fs::copy(&file, root.path().join("unindexed.FLAC")).unwrap();
    let mut missing = Track {
        path: Some("missing.mp3".into()),
        ..Default::default()
    };
    db.insert_track(&mut missing).unwrap();
    let report = synchronize(
        &db,
        root.path(),
        DurationUnit::Milliseconds,
        &tools,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(
        (
            report.updated_count,
            report.missing_count,
            report.unindexed_count
        ),
        (1, 1, 1)
    );
    assert_eq!(report.unindexed_paths, ["unindexed.FLAC"]);
    assert_eq!(db.track(existing.id).unwrap().unwrap().rating, Some(5));
}

#[tokio::test]
async fn move_never_overwrites_and_database_failure_retains_original_file() {
    let root = support::scratch();
    let db = database();
    let cancel = CancellationToken::new();
    let tools = tools(root.path());
    let file = root.path().join("tone.flac");
    audio(&file);
    let added = import(
        &db,
        root.path(),
        &file,
        &ImportOptions {
            move_file: true,
            ..Default::default()
        },
        &tools,
        &cancel,
    )
    .await
    .unwrap();
    assert!(added.moved);
    assert!(!file.exists());
    let original = root.path().join("Music/tone.flac");
    let bytes = std::fs::read(&original).unwrap();
    let target = root.path().join("occupied.flac");
    std::fs::write(&target, b"other audio").unwrap();
    assert!(
        move_file(
            &db,
            root.path(),
            &added.track.id.to_string(),
            &target,
            &cancel
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"other audio");
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    db.connection().execute_batch("CREATE TRIGGER reject_move BEFORE UPDATE ON Track BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
    let target = root.path().join("moved.flac");
    assert!(
        move_file(
            &db,
            root.path(),
            &added.track.id.to_string(),
            &target,
            &cancel
        )
        .is_err()
    );
    assert!(original.exists());
    assert!(!target.exists());
    db.connection()
        .execute_batch("DROP TRIGGER reject_move;")
        .unwrap();
    let moved = move_file(
        &db,
        root.path(),
        &added.track.id.to_string(),
        &target,
        &cancel,
    )
    .unwrap();
    assert_eq!(moved.path.as_deref(), Some("moved.flac"));
    assert!(!original.exists());
    remove(&db, root.path(), &moved.id.to_string(), true, &cancel).unwrap();
    assert!(!target.exists());
    assert!(db.tracks().unwrap().is_empty());
}

#[tokio::test]
async fn fallback_probe_reports_warning_and_cancelled_import_does_not_mutate() {
    let root = support::scratch();
    let db = database();
    let cancel = CancellationToken::new();
    let file = root.path().join("unprobed.flac");
    std::fs::write(&file, b"unreadable audio").unwrap();
    let tools = Tools::new("".into(), root.path().into());
    let result = import(
        &db,
        root.path(),
        &file,
        &ImportOptions::default(),
        &tools,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(result.track.title.as_deref(), Some("unprobed"));
    assert!(result.probe_warning.is_some());
    cancel.cancel();
    assert!(matches!(
        import(
            &db,
            root.path(),
            &file,
            &ImportOptions::default(),
            &tools,
            &cancel
        )
        .await,
        Err(Error::Cancelled)
    ));
    assert_eq!(db.tracks().unwrap().len(), 1);
}

#[tokio::test]
async fn import_failure_rolls_back_staged_file_and_sync_keeps_unreadable_metadata() {
    let root = support::scratch();
    let db = database();
    let file = root.path().join("unprobed.flac");
    std::fs::write(&file, b"audio").unwrap();
    let token = CancellationToken::new();
    let tools = Tools::new("".into(), root.path().into());
    db.connection().execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON Track BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
    assert!(
        import(
            &db,
            root.path(),
            &file,
            &ImportOptions {
                move_file: true,
                ..Default::default()
            },
            &tools,
            &token
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"audio");
    assert!(!root.path().join("Music/unprobed.flac").exists());
    assert!(db.tracks().unwrap().is_empty());
    db.connection()
        .execute_batch("DROP TRIGGER reject_import;")
        .unwrap();
    for path in [None, Some(""), Some("Music"), Some("unprobed.flac")] {
        let mut track = Track {
            path: path.map(str::to_owned),
            title: Some("Retain".into()),
            ..Default::default()
        };
        db.insert_track(&mut track).unwrap();
    }
    let report = synchronize(&db, root.path(), DurationUnit::Seconds, &tools, &token)
        .await
        .unwrap();
    assert_eq!(
        (
            report.missing_count,
            report.updated_count,
            report.warnings.len()
        ),
        (3, 1, 1)
    );
    let track = db.find_track("unprobed.flac").unwrap().unwrap();
    assert_eq!(track.title.as_deref(), Some("Retain"));
    assert!(move_file(&db, root.path(), "missing", &file, &token).is_err());
    assert!(
        move_file(
            &db,
            root.path(),
            &track.id.to_string(),
            Path::new(""),
            &token
        )
        .is_err()
    );
    assert!(move_file(&db, root.path(), "1", &file, &token).is_err());
    assert!(remove(&db, root.path(), "missing", false, &token).is_err());
    assert!(
        import(
            &db,
            root.path(),
            root.path(),
            &ImportOptions::default(),
            &tools,
            &token
        )
        .await
        .is_err()
    );
    let directory_track = db
        .tracks()
        .unwrap()
        .into_iter()
        .find(|track| track.path.as_deref() == Some("Music"))
        .unwrap();
    assert!(matches!(
        remove(
            &db,
            root.path(),
            &directory_track.id.to_string(),
            true,
            &token
        ),
        Err(Error::CommittedCleanup { .. })
    ));
    assert!(db.track(directory_track.id).unwrap().is_none());
    let removed = remove(&db, root.path(), &track.id.to_string(), false, &token).unwrap();
    assert_eq!(removed.id, track.id);
    assert!(file.exists());
    let error = tools
        .run(
            "missing-program",
            &audio_metadata::process::args(&[]),
            10,
            std::time::Duration::from_secs(1),
            &token,
        )
        .await
        .unwrap_err();
    let native: Error = error.into();
    assert!(matches!(native, Error::Audio(_)));
    token.cancel();
    let native: Error = audio_metadata::Error::Cancelled.into();
    assert!(matches!(native, Error::Cancelled));
}

#[cfg(unix)]
#[tokio::test]
async fn non_utf8_database_paths_are_rejected_without_mangling_or_mutation() {
    use std::os::unix::ffi::OsStringExt;
    let root = support::scratch();
    let path = root
        .path()
        .join(std::ffi::OsString::from_vec(b"bad-\xff.flac".to_vec()));
    std::fs::write(&path, b"audio").unwrap();
    let db = database();
    let token = CancellationToken::new();
    assert!(matches!(
        import(
            &db,
            root.path(),
            &path,
            &ImportOptions::default(),
            &tools(root.path()),
            &token
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(db.tracks().unwrap().is_empty());
    assert!(path.exists());
}
