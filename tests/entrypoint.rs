use std::{path::Path, process::Command};

fn fixture() -> tempfile::TempDir {
    std::fs::create_dir_all(".tmp").unwrap();
    let root = tempfile::tempdir_in(".tmp").unwrap();
    std::fs::create_dir_all(root.path().join("library/Database2")).unwrap();
    let db = rusqlite::Connection::open(root.path().join("library/Database2/m.db")).unwrap();
    db.execute_batch(include_str!("schema.sql")).unwrap();
    root
}
fn command(root: &Path) -> Command {
    let root = root.canonicalize().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_engine-cli"));
    command
        .env("HOME", &root)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("AGENT", "1")
        .args(["--lib-dir"])
        .arg(root.join("library"));
    command
}
#[test]
fn safety_snapshots_precede_mutations_and_restore_preserves_current_state() {
    let root = fixture();
    let result = command(root.path())
        .args(["playlist", "create", "Saved"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let snapshot = output
        .lines()
        .find_map(|line| line.strip_prefix("backup_snapshot: "))
        .unwrap();
    assert!(
        snapshot.starts_with("~/data/engine-cli/backups/"),
        "{output}"
    );
    let snapshot = root.path().join(snapshot.trim_start_matches("~/"));
    let db = rusqlite::Connection::open(snapshot.join("Database2/m.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM Playlist", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let listed = command(root.path())
        .args(["db", "backup", "--list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    assert!(
        String::from_utf8(listed.stdout)
            .unwrap()
            .contains("count: 1")
    );
    let restored = command(root.path())
        .args(["lib", "restore"])
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let text = String::from_utf8(restored.stdout).unwrap();
    let safety = text
        .lines()
        .find_map(|line| line.strip_prefix("safety_backup: "))
        .unwrap();
    let db = rusqlite::Connection::open(
        root.path()
            .join(safety.trim_start_matches("~/"))
            .join("Database2/m.db"),
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM Playlist", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "Saved"
    );
    let listed = command(root.path())
        .args(["playlist", "list"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8(listed.stdout)
            .unwrap()
            .contains("status: empty")
    );
    let healthy = command(root.path())
        .args(["library", "verify"])
        .output()
        .unwrap();
    assert!(healthy.status.success());
    assert!(
        String::from_utf8(healthy.stdout)
            .unwrap()
            .contains("status: healthy")
    );
}
#[test]
fn entrypoint_separates_offline_validation_runtime_errors_and_raw_metadata() {
    let root = fixture();
    for agent in ["0", "1", "true", "yes"] {
        let version = command(root.path())
            .env("AGENT", agent)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        assert_eq!(version.stdout, b"0.1.0\n");
        assert!(version.stderr.is_empty());
        let skill = command(root.path())
            .env("AGENT", agent)
            .arg("skill")
            .output()
            .unwrap();
        assert!(skill.status.success());
        assert_eq!(skill.stdout, engine_cli::cli::SKILL_POINTER.as_bytes());
    }
    for args in [
        vec!["track", "move", "1"],
        vec!["playlist", "move", "1"],
        vec!["track", "relocate"],
        vec!["--duration-unit", "guessed", "library", "verify"],
        vec!["skill", "extra"],
    ] {
        let error = command(root.path()).args(args).output().unwrap();
        assert_eq!(error.status.code(), Some(1));
        assert!(error.stdout.is_empty());
        assert!(String::from_utf8_lossy(&error.stderr).starts_with("error:"));
        assert!(!String::from_utf8_lossy(&error.stderr).contains("database"));
    }
    let error = command(root.path())
        .args(["--no-backup", "playlist", "inspect", "absent"])
        .output()
        .unwrap();
    assert_eq!(error.status.code(), Some(1));
    assert!(error.stdout.is_empty());
    let stderr = String::from_utf8(error.stderr).unwrap();
    assert_eq!(stderr.matches("ERR:").count(), 1, "{stderr}");
    assert!(!stderr.contains("Usage:"));
    let help = command(root.path())
        .args(["playlist", "track", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .starts_with("ALERT: Agents must read")
    );
    let human = command(root.path())
        .env("AGENT", "0")
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(human.stdout).unwrap();
    assert!(help.contains("├─") && help.contains("╰─"));
    assert!(!help.contains("ALERT:"));
}
