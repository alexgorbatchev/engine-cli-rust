mod support;
use engine_library::Error;
use engine_library::backup::{create, list, restore};
use rusqlite::Connection;
use std::fs;
use tokio_util::sync::CancellationToken;

#[test]
fn live_wal_backup_is_complete_unique_and_restore_replaces_database_contents() {
    let root = support::scratch();
    let library = root.path().join("library");
    let output = root.path().join("backups");
    fs::create_dir_all(library.join("Database2")).unwrap();
    fs::create_dir(library.join("Artwork")).unwrap();
    fs::write(library.join("Artwork/image"), b"picture").unwrap();
    let path = library.join("Database2/m.db");
    let live = Connection::open(&path).unwrap();
    live.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE data(value); INSERT INTO data VALUES ('committed in WAL');").unwrap();
    let cancel = CancellationToken::new();
    let first = create(&library, Some(&output), &cancel).unwrap();
    let second = create(&library, Some(&output), &cancel).unwrap();
    assert_ne!(first.path, second.path);
    assert_eq!((first.db_files, first.art_files), (1, 1));
    assert_eq!(list(&output).unwrap().len(), 2);
    let saved = Connection::open(first.path.join("Database2/m.db")).unwrap();
    assert_eq!(
        saved
            .query_row("SELECT value FROM data", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "committed in WAL"
    );
    live.execute("INSERT INTO data VALUES ('later')", [])
        .unwrap();
    fs::write(library.join("Artwork/image"), b"changed").unwrap();
    restore(&first.path, &library, &cancel).unwrap();
    assert_eq!(
        live.query_row("SELECT count(*) FROM data", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(fs::read(library.join("Artwork/image")).unwrap(), b"picture");
    assert!(first.size_bytes > 7);
}

#[test]
fn invalid_or_cancelled_backup_never_publishes_partial_snapshot() {
    let root = support::scratch();
    let library = root.path().join("library");
    let output = root.path().join("backups");
    fs::create_dir_all(&library).unwrap();
    fs::write(library.join("invalid.db"), b"not sqlite").unwrap();
    let cancel = CancellationToken::new();
    assert!(create(&library, Some(&output), &cancel).is_err());
    assert!(list(&output).unwrap().is_empty());
    cancel.cancel();
    assert!(matches!(
        create(&library, Some(&output), &cancel),
        Err(Error::Cancelled)
    ));
    assert!(list(&root.path().join("missing")).unwrap().is_empty());
    assert!(restore(&library, &library, &CancellationToken::new()).is_err());
}

#[test]
fn restore_preflights_all_databases_before_modifying_destination() {
    let root = support::scratch();
    let library = root.path().join("library");
    let saved = root.path().join("saved");
    fs::create_dir_all(&library).unwrap();
    fs::create_dir_all(saved.join("Root")).unwrap();
    let live = Connection::open(library.join("a.db")).unwrap();
    live.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES(2);")
        .unwrap();
    let source = Connection::open(saved.join("Root/a.db")).unwrap();
    source
        .execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES(1);")
        .unwrap();
    fs::write(saved.join("Root/z.db"), b"broken").unwrap();
    assert!(restore(&saved, &library, &CancellationToken::new()).is_err());
    assert_eq!(
        live.query_row("SELECT x FROM t", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn large_backup_steps_and_restore_partial_failure_keep_completed_paths() {
    let root = support::scratch();
    let library = root.path().join("library");
    fs::create_dir_all(library.join("Database2")).unwrap();
    let database = Connection::open(library.join("Database2/m.db")).unwrap();
    database
        .execute_batch("CREATE TABLE data(value); INSERT INTO data VALUES(zeroblob(1048576));")
        .unwrap();
    let other = Connection::open(library.join("root.db")).unwrap();
    other
        .execute_batch("CREATE TABLE other(value); INSERT INTO other VALUES(42);")
        .unwrap();
    fs::write(library.join("ignore.txt"), b"ignored").unwrap();
    fs::create_dir(library.join("directory.db")).unwrap();
    let token = CancellationToken::new();
    let saved = create(&library, Some(&root.path().join("backups")), &token).unwrap();
    assert_eq!(saved.db_files, 2);
    let target = root.path().join("target");
    fs::create_dir_all(target.join("root.db")).unwrap();
    let error = restore(&saved.path, &target, &token).unwrap_err();
    let Error::PartialRestore { completed, .. } = error else {
        panic!("expected partial restore: {error}");
    };
    assert_eq!(completed, [target.join("Database2/m.db")]);
    assert_eq!(
        Connection::open(&completed[0])
            .unwrap()
            .query_row("SELECT length(value) FROM data", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1048576
    );
    assert!(
        engine_library::backup::automatic(&library, true, &token)
            .unwrap()
            .is_none()
    );
    let empty = root.path().join("empty");
    fs::create_dir(&empty).unwrap();
    assert!(restore(&empty, &target, &token).is_err());
    assert!(create(&root.path().join("absent"), None, &token).is_err());
    assert!(list(&library.join("ignore.txt")).is_err());
}
