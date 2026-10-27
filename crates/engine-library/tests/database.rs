mod support;
use engine_library::{Database, Error, Track};
use rusqlite::Connection;

fn fixture() -> (tempfile::TempDir, Database) {
    std::fs::create_dir_all(".tmp").unwrap();
    let dir = support::scratch();
    let connection = Connection::open(dir.path().join("m.db")).unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    (dir, Database::from_connection(connection).unwrap())
}

fn track(database: &Database, title: &str) -> Track {
    let mut track = Track {
        title: Some(title.into()),
        path: Some(format!("Music/{title}.mp3")),
        filename: Some(format!("{title}.mp3")),
        ..Track::default()
    };
    database.insert_track(&mut track).unwrap();
    track
}

#[test]
fn native_nullable_crud_and_literal_path_resolution() {
    let (_dir, database) = fixture();
    let mut track = track(&database, "A_%");
    assert_eq!(database.track(track.id).unwrap().unwrap(), track);
    assert!(database.track(900).unwrap().is_none());
    assert_eq!(database.find_track("a_%").unwrap().unwrap().id, track.id);
    assert!(database.find_track("A_").unwrap().is_none());
    track.artist = Some("DJ".into());
    track.file_bytes = Some(42);
    database.update_track(&track).unwrap();
    assert_eq!(database.tracks().unwrap()[0].artist.as_deref(), Some("DJ"));
    assert!(database.find_track("").is_err());
    assert!(
        database
            .update_track(&Track {
                id: 900,
                ..track.clone()
            })
            .is_err()
    );
    assert!(database.delete_track(900).is_err());
    database.delete_track(track.id).unwrap();
    assert!(database.tracks().unwrap().is_empty());
}

#[test]
fn playlist_membership_move_is_atomic_and_delete_relinks() {
    let (_dir, database) = fixture();
    let source = database.create_playlist("Source", 0, false).unwrap();
    let destination = database.create_playlist("Destination", 0, false).unwrap();
    let a = track(&database, "A");
    let b = track(&database, "B");
    let c = track(&database, "C");
    let first = database.add_membership(source.id, a.id).unwrap();
    database.add_membership(source.id, b.id).unwrap();
    database.add_membership(source.id, c.id).unwrap();
    assert_eq!(database.add_membership(source.id, a.id).unwrap(), first);
    database.connection().execute_batch("CREATE TRIGGER fail_destination BEFORE INSERT ON PlaylistEntity WHEN NEW.listId=2 BEGIN SELECT RAISE(ABORT,'injected native failure'); END;").unwrap();
    assert!(
        database
            .move_membership(source.id, destination.id, b.id)
            .is_err()
    );
    assert_eq!(
        database
            .playlist_tracks(source.id)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        vec![a.id, b.id, c.id]
    );
    database
        .connection()
        .execute_batch("DROP TRIGGER fail_destination;")
        .unwrap();
    database.delete_track(b.id).unwrap();
    assert_eq!(
        database.entities(source.id).unwrap()[0].next_entity_id,
        database.entities(source.id).unwrap()[1].id
    );
    database
        .move_membership(source.id, destination.id, a.id)
        .unwrap();
    assert_eq!(
        database.playlist_tracks(destination.id).unwrap()[0].id,
        a.id
    );
    database
        .move_membership(destination.id, destination.id, a.id)
        .unwrap();
    database.remove_membership(destination.id, a.id).unwrap();
    assert!(database.remove_membership(destination.id, a.id).is_err());
    assert!(database.add_membership(900, a.id).is_err());
    assert!(database.add_membership(source.id, 900).is_err());
}

#[test]
fn hierarchy_cycles_ambiguous_names_and_recursive_delete() {
    let (_dir, database) = fixture();
    let parent = database.create_playlist("Root", 0, true).unwrap();
    let child = database
        .create_playlist("Nested", parent.id, false)
        .unwrap();
    assert!(database.move_playlist(parent.id, child.id).is_err());
    assert!(database.delete_playlist(parent.id, false).is_err());
    assert!(database.create_playlist(" ", 0, false).is_err());
    assert!(database.create_playlist("Absent", 900, false).is_err());
    assert!(database.move_playlist(child.id, 900).is_err());
    database.move_playlist(child.id, 0).unwrap();
    database.create_playlist("Nested", 0, false).unwrap();
    assert!(matches!(
        database.find_playlist("nested"),
        Err(Error::Ambiguous { .. })
    ));
    assert_eq!(
        database
            .find_playlist(&parent.id.to_string())
            .unwrap()
            .unwrap()
            .is_folder,
        Some(true)
    );
    assert!(database.find_playlist("missing").unwrap().is_none());
    assert!(database.find_playlist("").is_err());
    database.move_playlist(child.id, parent.id).unwrap();
    database.delete_playlist(parent.id, true).unwrap();
    assert!(
        database
            .find_playlist(&child.id.to_string())
            .unwrap()
            .is_none()
    );
    assert!(database.delete_playlist(900, true).is_err());
    assert!(database.move_playlist(900, 0).is_err());
}

#[test]
fn relocation_changes_only_complete_directory_prefix_and_art_crud() {
    let (_dir, database) = fixture();
    let mut a = track(&database, "a");
    a.path = Some("/old/Music/a.mp3".into());
    database.update_track(&a).unwrap();
    let mut b = track(&database, "b");
    b.path = Some("/older/b.mp3".into());
    database.update_track(&b).unwrap();
    assert_eq!(database.relocate_tracks("/old", "/new").unwrap(), 1);
    assert_eq!(
        database.track(a.id).unwrap().unwrap().path.as_deref(),
        Some("/new/Music/a.mp3")
    );
    assert_eq!(database.track(b.id).unwrap().unwrap().path, b.path);
    assert!(database.relocate_tracks("", "/new").is_err());
    let id = database.upsert_art("hash", b"first").unwrap();
    assert_eq!(database.upsert_art("hash", b"second").unwrap(), id);
    database.update_art(id, "other", b"third").unwrap();
    assert_eq!(database.artwork().unwrap()[0].data, b"third");
    database.delete_art(id).unwrap();
    assert!(database.artwork().unwrap().is_empty());
}

#[test]
fn native_schema_without_optional_columns_is_preserved_and_readonly_is_enforced() {
    let (dir, database) = fixture();
    database.connection().execute_batch("ALTER TABLE Track DROP COLUMN albumArtSourceHash; ALTER TABLE Playlist DROP COLUMN isFolder;").unwrap();
    drop(database);
    let database = Database::open(dir.path().join("m.db"), false).unwrap();
    assert!(!database.has_folder_flag());
    assert!(matches!(
        database.create_playlist("Folder", 0, true),
        Err(Error::Schema(_))
    ));
    let playlist = database.create_playlist("Container", 0, false).unwrap();
    assert_eq!(playlist.is_folder, None);
    let mut a = track(&database, "Native");
    a.album_art_source_hash = Some("ignored absent optional column".into());
    database.update_track(&a).unwrap();
    assert_eq!(
        database.track(a.id).unwrap().unwrap().album_art_source_hash,
        None
    );
    assert_eq!(
        database.find_track(&a.id.to_string()).unwrap().unwrap().id,
        a.id
    );
    let readonly = Database::open(dir.path().join("m.db"), true).unwrap();
    assert_eq!(readonly.tracks().unwrap().len(), 1);
    assert!(readonly.create_playlist("Denied", 0, false).is_err());
    assert!(Database::open(dir.path().join("missing.db"), false).is_err());
    assert!(!dir.path().join("missing.db").exists());
    assert!(Database::from_connection(Connection::open_in_memory().unwrap()).is_err());
}

#[test]
fn recursive_delete_respects_native_parent_constraints_and_duplicate_memberships_are_relinked() {
    let (_dir, database) = fixture();
    let parent = database.create_playlist("Parent", 0, true).unwrap();
    database.create_playlist("Child", parent.id, false).unwrap();
    database.connection().execute_batch("CREATE TRIGGER reject_parent_delete BEFORE DELETE ON Playlist WHEN EXISTS(SELECT 1 FROM Playlist WHERE parentListId=OLD.id) BEGIN SELECT RAISE(ABORT,'parent remains referenced'); END;").unwrap();
    database.delete_playlist(parent.id, true).unwrap();
    assert!(database.playlists().unwrap().is_empty());
    let list = database.create_playlist("List", 0, false).unwrap();
    let a = track(&database, "A");
    let b = track(&database, "B");
    let first = database.add_membership(list.id, a.id).unwrap();
    let second = database.add_membership(list.id, b.id).unwrap();
    database
        .connection()
        .execute(
            "INSERT INTO PlaylistEntity(listId,trackId,nextEntityId) VALUES (?,?,0)",
            [list.id, a.id],
        )
        .unwrap();
    let duplicate = database.connection().last_insert_rowid();
    database
        .connection()
        .execute(
            "UPDATE PlaylistEntity SET nextEntityId=? WHERE id=?",
            [duplicate, second],
        )
        .unwrap();
    database.delete_track(a.id).unwrap();
    assert_eq!(database.entities(list.id).unwrap().len(), 1);
    assert_eq!(database.entities(list.id).unwrap()[0].next_entity_id, 0);
    assert_eq!(database.playlist_tracks(list.id).unwrap()[0].id, b.id);
    assert!(
        !database
            .entities(list.id)
            .unwrap()
            .iter()
            .any(|entity| entity.id == first)
    );
}
