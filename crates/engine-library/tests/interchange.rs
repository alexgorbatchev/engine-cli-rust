use engine_library::{Database, Playlist, Track, records::TrackRecord};

#[test]
fn explicit_playlist_identity_appends_native_links_and_rejects_invalid_parents() {
    std::fs::create_dir_all(".tmp").unwrap();
    let dir = tempfile::tempdir_in(".tmp").unwrap();
    let db = Database::create(dir.path().join("m.db")).unwrap();
    db.insert_playlist_record(&Playlist {
        id: 40,
        title: "Parent".into(),
        parent_list_id: 0,
        is_folder: None,
    })
    .unwrap();
    db.insert_playlist_record(&Playlist {
        id: 6,
        title: "Child".into(),
        parent_list_id: 40,
        is_folder: None,
    })
    .unwrap();
    let next = db.create_playlist("Next", 0, false).unwrap();
    assert!(next.id > 40);
    assert_eq!(
        db.playlist_siblings(0)
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [40, next.id]
    );
    for playlist in [
        Playlist {
            id: 0,
            title: "Invalid".into(),
            parent_list_id: 0,
            is_folder: None,
        },
        Playlist {
            id: 7,
            title: "Orphan".into(),
            parent_list_id: 999,
            is_folder: None,
        },
        Playlist {
            id: 40,
            title: "Duplicate".into(),
            parent_list_id: 0,
            is_folder: None,
        },
    ] {
        assert!(db.insert_playlist_record(&playlist).is_err());
    }
    assert_eq!(db.playlists().unwrap().len(), 3);
    db.transaction(|db| {
        db.insert_playlist_record(&Playlist {
            id: 77,
            title: "Transactional".into(),
            parent_list_id: 40,
            is_folder: None,
        })
    })
    .unwrap();
    assert_eq!(db.require_playlist(77).unwrap().parent_list_id, 40);
    let result: engine_library::Result<()> = db.transaction(|db| {
        db.insert_playlist_record(&Playlist {
            id: 78,
            title: "Rollback".into(),
            parent_list_id: 40,
            is_folder: None,
        })?;
        Err(engine_library::Error::Invalid(
            "abort collection edit".into(),
        ))
    });
    assert!(result.is_err());
    assert!(db.require_playlist(78).is_err());
}

#[test]
fn history_append_links_native_track_and_accepts_unknown_play_time() {
    std::fs::create_dir_all(".tmp").unwrap();
    let dir = tempfile::tempdir_in(".tmp").unwrap();
    let db = Database::create_history(dir.path().join("hm.db")).unwrap();
    db.put_track_record(&TrackRecord {
        track: Track {
            id: 4,
            ..Track::default()
        },
        ..TrackRecord::default()
    })
    .unwrap();
    engine_library::history::append(db.connection(), 4, None).unwrap();
    let time = chrono::DateTime::from_timestamp(1700000000, 0).unwrap();
    engine_library::history::append(db.connection(), 4, Some(time)).unwrap();
    assert!(engine_library::history::append(db.connection(), 999, None).is_err());
    let history = engine_library::history::read(db.connection()).unwrap();
    assert_eq!(history[0].plays, 2);
    assert_eq!(history[0].last_played, Some(time));
}
