use engine_library::performance::PerformanceData;
use engine_library::records::{PlaylistMetadata, Smartlist, TrackMetadata, TrackRecord};
use engine_library::{Database, Track};

fn database() -> (tempfile::TempDir, Database) {
    std::fs::create_dir_all(".tmp").unwrap();
    let dir = tempfile::tempdir_in(".tmp").unwrap();
    let db = Database::create(dir.path().join("m.db")).unwrap();
    (dir, db)
}

#[test]
fn explicit_creation_never_replaces_an_existing_database_and_generates_unique_identity() {
    let (dir, db) = database();
    let info = db.information().unwrap().unwrap();
    assert!(!info.uuid.is_nil());
    assert_eq!(info.schema_version, [2, 20, 0]);
    assert!(Database::create(dir.path().join("m.db")).is_err());
    assert_eq!(db.information().unwrap().unwrap(), info);
    let (_, other) = database();
    assert_ne!(other.information().unwrap().unwrap().uuid, info.uuid);
}

#[test]
fn full_native_records_preserve_nulls_origin_ids_and_timestamps_across_existing_crud() {
    let (_dir, db) = database();
    let record = TrackRecord {
        track: Track {
            id: 40,
            title: Some("Track".into()),
            path: Some("Music/a.mp3".into()),
            last_edit_time: Some("2020-01-02T03:04:05Z".into()),
            ..Track::default()
        },
        metadata: TrackMetadata {
            origin_track_id: Some(7),
            origin_database_uuid: Some("original-db".into()),
            is_played: Some(true),
            date_added: Some("2021-02-03T00:00:00Z".into()),
            streaming_flags: Some(42),
            ..TrackMetadata::default()
        },
    };
    db.put_track_record(&record).unwrap();
    assert_eq!(db.track_record(40).unwrap().unwrap(), record);
    let mut track = db.track(40).unwrap().unwrap();
    track.title = Some("Updated".into());
    db.update_track(&track).unwrap();
    assert_eq!(
        db.track_record(40).unwrap().unwrap().metadata,
        record.metadata
    );
    assert!(db.track_record(999).unwrap().is_none());
    assert_eq!(db.track_records().unwrap().len(), 1);
    let mut perf = PerformanceData {
        track_id: 40,
        third_party_source_id: Some(77),
        active_on_load_loops: Some(2),
        ..PerformanceData::default()
    };
    perf.track_data = Some(vec![1, 2]);
    db.put_performance(&perf).unwrap();
    assert_eq!(db.performance(40).unwrap(), Some(perf));
    db.delete_track(40).unwrap();
    assert!(db.performance(40).unwrap().is_none());
    assert!(
        db.put_performance(&PerformanceData {
            track_id: 999,
            ..PerformanceData::default()
        })
        .is_err()
    );
}

#[test]
fn playlist_order_and_membership_use_native_database_identity() {
    let (_dir, db) = database();
    let a = db.create_playlist("First", 0, false).unwrap();
    let b = db.create_playlist("Second", 0, false).unwrap();
    db.put_playlist_metadata(
        a.id,
        &PlaylistMetadata {
            next_list_id: Some(b.id),
            is_persisted: Some(true),
            ..PlaylistMetadata::default()
        },
    )
    .unwrap();
    assert_eq!(db.playlist_metadata(a.id).unwrap().next_list_id, Some(b.id));
    let mut track = Track::default();
    db.insert_track(&mut track).unwrap();
    let entity = db.add_membership(a.id, track.id).unwrap();
    let uuid: String = db
        .connection()
        .query_row(
            "SELECT databaseUuid FROM PlaylistEntity WHERE id=?",
            [entity],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(uuid, db.information().unwrap().unwrap().uuid.to_string());
    let rule = Smartlist {
        list_uuid: "rules-1".into(),
        title: Some("New".into()),
        rules: Some("{\"filter\":\"house\"}".into()),
        ..Smartlist::default()
    };
    db.put_smartlist(&rule).unwrap();
    assert_eq!(db.smartlists().unwrap(), vec![rule]);
}

#[test]
fn malformed_optional_tables_fail_instead_of_silently_discarding_data() {
    let (_dir, db) = database();
    db.connection()
        .execute_batch("DROP TABLE Smartlist; CREATE TABLE Smartlist(listUuid TEXT);")
        .unwrap();
    assert!(db.smartlists().is_err());
    db.connection()
        .execute_batch("DROP TABLE Information; CREATE TABLE Information(uuid TEXT);")
        .unwrap();
    assert!(db.information().is_err());
}

#[test]
fn history_keeps_origin_namespaces_and_understands_native_epoch_dates() {
    let (_dir, db) = database();
    db.connection()
        .execute_batch("CREATE TABLE HistorylistEntity(trackId INTEGER,startTime DATETIME);")
        .unwrap();
    for (id, origin) in [(1, "database-a"), (2, "database-b")] {
        db.put_track_record(&TrackRecord {
            track: Track {
                id,
                ..Track::default()
            },
            metadata: TrackMetadata {
                origin_track_id: Some(7),
                origin_database_uuid: Some(origin.into()),
                ..TrackMetadata::default()
            },
        })
        .unwrap();
    }
    db.connection()
        .execute_batch(
            "INSERT INTO HistorylistEntity VALUES(1,1700000000),(1,1700000001),(2,1700000002);",
        )
        .unwrap();
    let entries = engine_library::history::read(db.connection()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].plays, 2);
    assert_eq!(entries[1].origin_track_id, 7);
    assert_eq!(entries[1].last_played.unwrap().timestamp(), 1700000002);
}

#[test]
fn history_compares_parsed_dates_instead_of_sqlite_storage_classes() {
    let (_dir, db) = database();
    db.connection().execute_batch("CREATE TABLE HistorylistEntity(trackId INTEGER,startTime DATETIME); INSERT INTO Track(id) VALUES(1); INSERT INTO HistorylistEntity VALUES(1,'2020-01-01'),(1,1700000002);").unwrap();
    let entries = engine_library::history::read(db.connection()).unwrap();
    assert_eq!(entries[0].plays, 2);
    assert_eq!(entries[0].last_played.unwrap().timestamp(), 1700000002);
}

#[test]
fn native_playlist_links_survive_reordering_moving_and_deleting_with_unique_constraints() {
    let (_dir, db) = database();
    db.connection()
        .execute_batch("CREATE UNIQUE INDEX sibling_next ON Playlist(parentListId,nextListId);")
        .unwrap();
    let a = db.create_playlist("A", 0, false).unwrap();
    let b = db.create_playlist("B", 0, false).unwrap();
    let c = db.create_playlist("C", 0, false).unwrap();
    // Updating A to point at C initially conflicts with B's current successor.
    db.reorder_playlists(0, &[a.id, c.id, b.id]).unwrap();
    db.reorder_playlists(0, &[c.id, a.id, b.id]).unwrap();
    assert_eq!(
        db.playlist_siblings(0)
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [c.id, a.id, b.id]
    );
    assert!(db.reorder_playlists(0, &[a.id, a.id, b.id]).is_err());
    assert!(db.reorder_playlists(0, &[a.id, b.id]).is_err());
    db.move_playlist(a.id, b.id).unwrap();
    assert_eq!(
        db.playlist_siblings(0)
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [c.id, b.id]
    );
    assert_eq!(db.playlist_siblings(b.id).unwrap()[0].id, a.id);
    db.delete_playlist(c.id, false).unwrap();
    assert_eq!(db.playlist_siblings(0).unwrap()[0].id, b.id);
    assert_eq!(db.playlist_metadata(b.id).unwrap().next_list_id, Some(0));
    let roots = engine_library::collection::hierarchy(&db).unwrap();
    assert_eq!(roots[0].children[0].id, a.id);
}

#[test]
fn older_native_schemas_preserve_core_crud_without_adding_optional_columns() {
    let (dir, db) = database();
    for name in ["fileType", "isAnalyzed", "isAvailable", "lastEditTime"] {
        db.connection()
            .execute_batch(&format!("ALTER TABLE Track DROP COLUMN {name}"))
            .unwrap();
    }
    drop(db);
    let db = Database::open(dir.path().join("m.db"), false).unwrap();
    let mut track = Track {
        title: Some("Old schema".into()),
        ..Track::default()
    };
    db.insert_track(&mut track).unwrap();
    track.title = Some("Updated".into());
    db.update_track(&track).unwrap();
    let loaded = db.track(track.id).unwrap().unwrap();
    assert_eq!(loaded.title.as_deref(), Some("Updated"));
    assert!(loaded.last_edit_time.is_none());
    assert_eq!(
        db.connection()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('Track') WHERE name='lastEditTime'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn import_artwork_retains_explicit_identity_and_null_hashes() {
    let (_dir, db) = database();
    let mut art = engine_library::AlbumArt {
        id: 42,
        hash: None,
        data: vec![1, 2, 3],
    };
    db.put_artwork(&art).unwrap();
    assert_eq!(db.artwork().unwrap(), vec![art.clone()]);
    art.data = vec![9];
    db.put_artwork(&art).unwrap();
    assert_eq!(db.artwork().unwrap()[0].data, vec![9]);
    art.id = 0;
    assert!(db.put_artwork(&art).is_err());
}

#[test]
fn native_timestamp_formats_and_invalid_dates_are_explicit() {
    use engine_library::records::parse_timestamp;
    let utc = chrono::DateTime::parse_from_rfc3339("2020-01-02T03:04:05Z")
        .unwrap()
        .to_utc();
    for text in [
        "2020-01-02T03:04:05Z",
        "2020-01-02 03:04:05",
        "2020-01-02T03:04:05",
        "2020-01-02 05:04:05+02:00",
        "2020-01-02 05:04:05+0200",
    ] {
        assert_eq!(parse_timestamp(text).unwrap(), Some(utc));
    }
    assert_eq!(
        parse_timestamp("2020-01-02").unwrap().unwrap().date_naive(),
        utc.date_naive()
    );
    assert_eq!(parse_timestamp("").unwrap(), None);
    assert_eq!(
        parse_timestamp("1700000000").unwrap().unwrap().timestamp(),
        1700000000
    );
    for text in ["NaN", "inf", "1e50", "2020-13-01", "not a date"] {
        assert!(parse_timestamp(text).is_err(), "{text}");
    }
}

#[test]
fn explicitly_created_history_stores_native_identity_and_absent_history_is_empty() {
    let (dir, db) = database();
    assert!(
        engine_library::history::read(db.connection())
            .unwrap()
            .is_empty()
    );
    let history = Database::create_history(dir.path().join("hm.db")).unwrap();
    history
        .put_track_record(&TrackRecord {
            track: Track {
                id: 6,
                ..Track::default()
            },
            ..TrackRecord::default()
        })
        .unwrap();
    history
        .connection()
        .execute(
            "INSERT INTO HistorylistEntity(trackId,startTime) VALUES(?,?)",
            rusqlite::params![6, "2020-01-01"],
        )
        .unwrap();
    let entries = engine_library::history::read(history.connection()).unwrap();
    assert_eq!(entries[0].origin_track_id, 6);
    assert_eq!(entries[0].last_played.unwrap().timestamp(), 1577836800);
}
