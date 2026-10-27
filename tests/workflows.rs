use cli_command_kit::Mode;
use engine_cli::{CancellationToken, cli, execute};
use std::path::PathBuf;

fn fixture() -> tempfile::TempDir {
    std::fs::create_dir_all(".tmp").unwrap();
    let root = tempfile::tempdir_in(".tmp").unwrap();
    std::fs::create_dir_all(root.path().join("Database2")).unwrap();
    let db = rusqlite::Connection::open(root.path().join("Database2/m.db")).unwrap();
    db.execute_batch(include_str!("schema.sql")).unwrap();
    db.execute_batch("INSERT INTO Track(id,title,artist,length,path,filename) VALUES(1,'Native song','Artist',90000,'missing.flac','missing.flac')").unwrap();
    root
}
async fn run(root: &std::path::Path, args: &[&str], mode: Mode) -> engine_cli::Result<String> {
    let mut argv = vec![
        "engine-cli".into(),
        "--lib-dir".into(),
        root.as_os_str().to_owned(),
        "--no-backup".into(),
    ];
    argv.extend(args.iter().map(std::ffi::OsString::from));
    let matches = cli::command(mode).try_get_matches_from(argv).unwrap();
    let mut output = Vec::new();
    execute(
        &matches,
        mode,
        &mut output,
        &mut Vec::new(),
        &CancellationToken::new(),
    )
    .await?;
    Ok(String::from_utf8(output).unwrap())
}
#[tokio::test]
async fn actual_membership_workflow_and_explicit_duration_units() {
    let root = fixture();
    assert!(
        run(
            root.path(),
            &["playlist", "create", "House", "--folder"],
            Mode::Agent
        )
        .await
        .unwrap()
        .contains("is_folder: true")
    );
    run(
        root.path(),
        &["playlist", "create", "Set", "--parent", "House"],
        Mode::Human,
    )
    .await
    .unwrap();
    run(
        root.path(),
        &["playlist", "track", "add", "Set", "1"],
        Mode::Agent,
    )
    .await
    .unwrap();
    assert!(
        run(root.path(), &["playlist", "inspect", "Set"], Mode::Agent)
            .await
            .unwrap()
            .contains("Artist - Native song [id:1 1:30]")
    );
    assert!(
        run(
            root.path(),
            &["--duration-unit", "seconds", "playlist", "inspect", "Set"],
            Mode::Human
        )
        .await
        .unwrap()
        .contains("1500:00")
    );
    let tree = run(root.path(), &["playlist", "list"], Mode::Agent)
        .await
        .unwrap();
    assert!(
        tree.contains("- House [id:1 tracks:0]\n    - Set [id:2 tracks:1]"),
        "{tree}"
    );
    run(
        root.path(),
        &["playlist", "move", "Set", "--to", "Collection"],
        Mode::Agent,
    )
    .await
    .unwrap();
    assert!(
        run(root.path(), &["playlist", "rm", "House"], Mode::Agent)
            .await
            .unwrap()
            .contains("deleted_id: 1")
    );
    run(root.path(), &["playlist", "create", "Other"], Mode::Agent)
        .await
        .unwrap();
    run(
        root.path(),
        &["playlist", "track", "move", "Set", "Other", "1"],
        Mode::Agent,
    )
    .await
    .unwrap();
    assert!(
        run(root.path(), &["playlist", "inspect", "Set"], Mode::Human)
            .await
            .unwrap()
            .contains("No tracks")
    );
    run(
        root.path(),
        &["playlist", "track", "rm", "Other", "1"],
        Mode::Agent,
    )
    .await
    .unwrap();
    assert!(
        run(root.path(), &["playlist", "inspect", "Other"], Mode::Agent)
            .await
            .unwrap()
            .contains("status: empty")
    );
}
#[tokio::test]
async fn native_verify_backup_restore_and_prune() {
    let root = fixture();
    let report = run(root.path(), &["library", "verify", "--json"], Mode::Agent)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(json["missingOnDiskCount"], 1);
    let output = root.path().join("snapshots");
    let backup = run(
        root.path(),
        &["library", "backup", output.to_str().unwrap()],
        Mode::Agent,
    )
    .await
    .unwrap();
    let path = engine_cli::output::expand(&PathBuf::from(
        backup
            .lines()
            .find_map(|l| l.strip_prefix("path: "))
            .unwrap(),
    ));
    assert!(path.join("Database2/m.db").is_file());
    assert!(
        run(
            root.path(),
            &[
                "library",
                "backup",
                "--list",
                "-o",
                output.to_str().unwrap()
            ],
            Mode::Agent
        )
        .await
        .unwrap()
        .contains("count: 1")
    );
    run(root.path(), &["track", "rm", "1"], Mode::Agent)
        .await
        .unwrap();
    run(
        root.path(),
        &["library", "restore", path.to_str().unwrap()],
        Mode::Agent,
    )
    .await
    .unwrap();
    assert!(
        run(root.path(), &["library", "verify"], Mode::Human)
            .await
            .unwrap()
            .contains("Missing Audio Files: 1")
    );
    assert!(
        run(root.path(), &["library", "prune"], Mode::Agent)
            .await
            .unwrap()
            .contains("deleted_db_rows: 0")
    );
}

#[tokio::test]
async fn native_audio_import_sync_move_relocate_and_artwork_reporting() {
    let root = fixture();
    let source = root.path().join("native.flac");
    let generated = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.4",
            "-metadata",
            "title=Imported native",
            "-metadata",
            "artist=Native artist",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let imported = run(
        root.path(),
        &[
            "track",
            "add",
            source.to_str().unwrap(),
            "--move",
            "--to-playlist",
            "Audio",
        ],
        Mode::Agent,
    )
    .await
    .unwrap();
    assert!(imported.contains("moved_file: true"), "{imported}");
    assert!(!source.exists());
    let moved = root.path().join("Music/renamed.flac");
    run(
        root.path(),
        &["track", "move", "Imported native", moved.to_str().unwrap()],
        Mode::Human,
    )
    .await
    .unwrap();
    assert!(moved.is_file());
    let synced = run(root.path(), &["library", "sync"], Mode::Agent)
        .await
        .unwrap();
    assert!(synced.contains("total_tracks: 2"));
    let unindexed = root.path().join("unindexed.wav");
    std::fs::write(&unindexed, b"not audio").unwrap();
    let sync = run(root.path(), &["library", "sync"], Mode::Human)
        .await
        .unwrap();
    assert!(sync.contains("unindexed files: 1"), "{sync}");
    let normalized = run(root.path(), &["track", "artwork", "normalize"], Mode::Agent)
        .await
        .unwrap();
    assert!(normalized.contains("status: complete"));
    assert!(normalized.contains("no_art: 1"), "{normalized}");
    assert!(normalized.contains("errors: 1"), "{normalized}");
    let relocated = run(
        root.path(),
        &["track", "relocate", "--from", "Music", "--to", "Moved"],
        Mode::Human,
    )
    .await
    .unwrap();
    assert!(relocated.contains("relocated count: 1"), "{relocated}");
    run(
        root.path(),
        &["track", "relocate", "--from", "Moved", "--to", "Music"],
        Mode::Agent,
    )
    .await
    .unwrap();
    run(
        root.path(),
        &["track", "rm", "Imported native", "--delete-file"],
        Mode::Human,
    )
    .await
    .unwrap();
    assert!(!moved.exists());
}

#[tokio::test]
async fn read_only_views_empty_groups_errors_and_output_failures() {
    let root = fixture();
    for mode in [Mode::Agent, Mode::Human] {
        assert!(
            run(root.path(), &["playlist", "list"], mode)
                .await
                .unwrap()
                .contains(if mode == Mode::Agent {
                    "status: empty"
                } else {
                    "No playlists"
                })
        );
        run(root.path(), &["library", "verify"], mode)
            .await
            .unwrap();
        let empty = root.path().join("empty");
        run(
            root.path(),
            &["library", "backup", "--list", "-o", empty.to_str().unwrap()],
            mode,
        )
        .await
        .unwrap();
        for args in [
            &[][..],
            &["library"][..],
            &["track"][..],
            &["track", "artwork"][..],
            &["playlist", "track"][..],
        ] {
            assert!(
                run(root.path(), args, mode)
                    .await
                    .unwrap()
                    .contains("engine-cli")
            );
        }
    }
    run(root.path(), &["playlist", "create", "First"], Mode::Agent)
        .await
        .unwrap();
    run(root.path(), &["playlist", "create", "Second"], Mode::Agent)
        .await
        .unwrap();
    assert!(
        run(root.path(), &["playlist", "list"], Mode::Human)
            .await
            .unwrap()
            .contains("├── First")
    );
    assert!(
        run(
            root.path(),
            &["playlist", "inspect", "missing"],
            Mode::Agent
        )
        .await
        .is_err()
    );
    assert!(
        run(
            root.path(),
            &["playlist", "track", "add", "First", "absent-query-unique"],
            Mode::Agent
        )
        .await
        .is_err()
    );
    run(
        root.path(),
        &["playlist", "move", "Second", "--to", "First"],
        Mode::Human,
    )
    .await
    .unwrap();
    assert!(
        run(root.path(), &["playlist", "rm", "First"], Mode::Agent)
            .await
            .is_err()
    );
    run(
        root.path(),
        &["playlist", "rm", "First", "--force"],
        Mode::Human,
    )
    .await
    .unwrap();
    let matches = cli::command(Mode::Agent)
        .try_get_matches_from(["engine-cli", "library", "verify"])
        .unwrap();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let error = execute(
        &matches,
        Mode::Agent,
        &mut Vec::new(),
        &mut Vec::new(),
        &cancelled,
    )
    .await
    .unwrap_err();
    assert_eq!(engine_cli::exit_status(&error), 130);
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed output",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for arg in ["skill", "--version"] {
        let matches = cli::command(Mode::Agent)
            .try_get_matches_from(["engine-cli", arg])
            .unwrap();
        let error = execute(
            &matches,
            Mode::Agent,
            &mut Broken,
            &mut Vec::new(),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(engine_cli::exit_status(&error), 1);
    }
    assert_eq!(
        engine_cli::exit_status(
            &engine_library::Error::CommittedCleanup {
                path: root.path().into(),
                source: std::io::Error::other("cleanup")
            }
            .into()
        ),
        6
    );
}
