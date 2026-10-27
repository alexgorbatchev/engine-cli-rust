mod support;
use engine_library::{
    Database, Error, Track,
    collection::{hierarchy, inspect_playlist, verify},
    model::DurationUnit,
};
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

fn database() -> Database {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    Database::from_connection(connection).unwrap()
}

#[test]
fn reports_real_file_metadata_and_broken_references_without_treating_folders_as_empty() {
    let db = database();
    let home = support::scratch();
    std::fs::write(home.path().join("present.wav"), b"audio").unwrap();
    let mut present = Track {
        path: Some("present.wav".into()),
        file_bytes: Some(6),
        album_art_id: Some(99),
        ..Default::default()
    };
    db.insert_track(&mut present).unwrap();
    let mut missing = Track {
        path: Some("missing.wav".into()),
        ..Default::default()
    };
    db.insert_track(&mut missing).unwrap();
    let folder = db.create_playlist("folder", 0, true).unwrap();
    let empty = db.create_playlist("empty", folder.id, false).unwrap();
    db.connection().execute("INSERT INTO PlaylistEntity(listId,trackId,nextEntityId,membershipReference) VALUES(?,?,0,0)", [empty.id, 123]).unwrap();
    db.connection().execute("INSERT INTO PlaylistEntity(listId,trackId,nextEntityId,membershipReference) VALUES(456,?,0,0)", [present.id]).unwrap();
    let report = verify(&db, home.path(), &CancellationToken::new()).unwrap();
    assert_eq!((report.total_tracks, report.total_playlists), (2, 2));
    assert_eq!(
        (
            report.missing_on_disk_count,
            report.bytes_mismatch_count,
            report.missing_art_count,
            report.orphaned_count,
            report.empty_playlist_count
        ),
        (1, 1, 1, 2, 1)
    );
    assert!(!report.is_healthy);
    assert_eq!(report.issues.len(), 6);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        verify(&db, home.path(), &cancel),
        Err(Error::Cancelled)
    ));
}

#[test]
fn hierarchy_counts_native_order_and_details_convert_only_explicit_units() {
    let db = database();
    let folder = db.create_playlist("folder", 0, true).unwrap();
    let list = db.create_playlist("list", folder.id, false).unwrap();
    for length in [2500, 4500] {
        let mut track = Track {
            length: Some(length),
            ..Default::default()
        };
        db.insert_track(&mut track).unwrap();
        db.add_membership(list.id, track.id).unwrap();
    }
    let tree = hierarchy(&db).unwrap();
    assert_eq!(tree[0].children[0].track_count, 2);
    let detail = inspect_playlist(&db, "list", DurationUnit::Milliseconds).unwrap();
    assert_eq!((detail.track_count, detail.total_duration_seconds), (2, 7));
    assert_eq!(
        inspect_playlist(&db, "list", DurationUnit::Seconds)
            .unwrap()
            .total_duration_seconds,
        7000
    );
    assert!(inspect_playlist(&db, "missing", DurationUnit::Seconds).is_err());
    db.connection()
        .execute(
            "UPDATE Playlist SET parentListId=? WHERE id=?",
            [list.id, folder.id],
        )
        .unwrap();
    assert!(matches!(hierarchy(&db), Err(Error::Integrity(_))));
    db.connection()
        .execute(
            "UPDATE Playlist SET parentListId=999 WHERE id=?",
            [folder.id],
        )
        .unwrap();
    assert!(matches!(hierarchy(&db), Err(Error::Integrity(_))));
}
