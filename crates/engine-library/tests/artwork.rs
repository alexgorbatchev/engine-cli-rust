mod support;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use engine_library::{
    Database, Track,
    artwork::{normalize, prune},
};
use image::{DynamicImage, GenericImageView};
use lofty::{
    config::WriteOptions,
    file::TaggedFileExt,
    picture::{Picture, PictureType},
    prelude::TagExt,
    tag::Tag,
};
use rusqlite::Connection;
use std::process::Command;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn native_embedded_art_generates_thumbnail_and_high_resolution_without_audio_changes() {
    let root = support::scratch();
    let file = root.path().join("tone.flac");
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "sine=duration=0.1"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(output.status.success());
    let art = root.path().join("cover.png");
    DynamicImage::new_rgb8(80, 40).save(&art).unwrap();
    let token = CancellationToken::new();
    let mut tagged = lofty::read_from_path(&file).unwrap();
    let tag_type = tagged.primary_tag_type();
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let mut picture =
        Picture::from_reader(&mut std::io::Cursor::new(std::fs::read(art).unwrap())).unwrap();
    picture.set_pic_type(PictureType::CoverFront);
    let tag = tagged.primary_tag_mut().unwrap();
    tag.push_picture(picture);
    tag.save_to_path(&file, WriteOptions::default()).unwrap();
    let before = std::fs::read(&file).unwrap();
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    let db = Database::from_connection(connection).unwrap();
    let mut track = Track {
        path: Some("tone.flac".into()),
        title: Some("Keep".into()),
        ..Default::default()
    };
    db.insert_track(&mut track).unwrap();
    let cache = root.path().join("Artwork");
    let report = normalize(&db, root.path(), &cache, &token).unwrap();
    assert_eq!((report.normalized_count, report.error_count), (1, 0));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    let art = db.artwork().unwrap().remove(0);
    assert_eq!(
        image::load_from_memory(&art.data).unwrap().dimensions(),
        (256, 256)
    );
    let hash = art.hash.unwrap();
    assert_eq!(
        image::open(cache.join(format!("{hash}.jpg")))
            .unwrap()
            .dimensions(),
        (1400, 1400)
    );
    let bytes = std::fs::read(cache.join(format!("{hash}.jpg"))).unwrap();
    use sha1::Digest;
    assert_eq!(hex::encode(sha1::Sha1::digest(&bytes)), hash);
    let b64 = URL_SAFE_NO_PAD.encode(hex::decode(&hash).unwrap());
    assert_eq!(
        std::fs::read(cache.join(format!("{b64}.jpg"))).unwrap(),
        bytes
    );
    let updated = db.track(track.id).unwrap().unwrap();
    assert_eq!(updated.album_art_id, Some(art.id));
    assert_eq!(updated.title.as_deref(), Some("Keep"));
    let again = normalize(&db, root.path(), &cache, &token).unwrap();
    assert_eq!(again.normalized_count, 1);
    assert_eq!(db.artwork().unwrap().len(), 1);
}

#[tokio::test]
async fn existing_art_and_orphan_cache_normalize_and_case_sensitive_pruning_is_exact() {
    let root = support::scratch();
    let cache = root.path().join("Artwork");
    std::fs::create_dir(&cache).unwrap();
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    let db = Database::from_connection(connection).unwrap();
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut encoded)
        .encode_image(&DynamicImage::new_rgb8(60, 30))
        .unwrap();
    let art = db.upsert_art("old", &encoded).unwrap();
    let mut track = Track {
        album_art_id: Some(art),
        ..Default::default()
    };
    db.insert_track(&mut track).unwrap();
    std::fs::write(cache.join("unrelated.jpg"), &encoded).unwrap();
    let token = CancellationToken::new();
    let report = normalize(&db, root.path(), &cache, &token).unwrap();
    assert_eq!(
        (
            report.no_art_count,
            report.normalized_count,
            report.cache_normalized_count
        ),
        (1, 1, 1)
    );
    assert_eq!(
        image::load_from_memory(&db.artwork().unwrap()[0].data)
            .unwrap()
            .dimensions(),
        (256, 256)
    );
    let hash = db.artwork().unwrap()[0].hash.clone().unwrap();
    let b64 = URL_SAFE_NO_PAD.encode(hex::decode(&hash).unwrap());
    let wrong_case = b64.to_lowercase();
    assert_ne!(wrong_case, b64);
    std::fs::write(cache.join(format!("{wrong_case}.jpg")), b"wrong case").unwrap();
    db.upsert_art("unreferenced", b"orphan").unwrap();
    let report = prune(&db, &cache, &token).unwrap();
    assert_eq!((report.deleted_db_rows, report.deleted_disk_files), (1, 2));
    assert!(cache.join(format!("{b64}.jpg")).exists());
    assert!(!cache.join(format!("{wrong_case}.jpg")).exists());
    assert_eq!(
        db.track(track.id)
            .unwrap()
            .unwrap()
            .album_art_source_hash
            .as_deref(),
        Some(hash.as_str())
    );
}

#[test]
fn malformed_audio_images_and_filesystem_errors_are_reported_and_cancellation_stops_writes() {
    let root = support::scratch();
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    let db = Database::from_connection(connection).unwrap();
    for path in ["missing.mp3", "bad.flac"] {
        let mut track = Track {
            path: Some(path.into()),
            ..Default::default()
        };
        db.insert_track(&mut track).unwrap();
    }
    std::fs::write(root.path().join("bad.flac"), b"not audio").unwrap();
    db.upsert_art("broken", b"not image").unwrap();
    db.upsert_art("empty", b"").unwrap();
    let cache = root.path().join("Artwork");
    std::fs::create_dir(&cache).unwrap();
    std::fs::create_dir(cache.join("ignored directory")).unwrap();
    std::fs::write(cache.join("broken.JPG"), b"not image").unwrap();
    std::fs::write(cache.join("preserve.txt"), b"nonimage").unwrap();
    let cancel = CancellationToken::new();
    let report = normalize(&db, root.path(), &cache, &cancel).unwrap();
    assert_eq!(report.error_count, 4);
    assert_eq!(report.warnings.len(), 4);
    assert!(normalize(&db, std::path::Path::new(""), &cache, &cancel).is_err());
    assert!(normalize(&db, root.path(), &cache.join("preserve.txt"), &cancel).is_err());
    let no_cache = std::path::Path::new("");
    assert_eq!(prune(&db, no_cache, &cancel).unwrap().deleted_db_rows, 2);
    assert_eq!(
        prune(&db, &root.path().join("absent"), &cancel)
            .unwrap()
            .deleted_disk_files,
        0
    );
    assert!(prune(&db, &cache.join("preserve.txt"), &cancel).is_err());
    cancel.cancel();
    assert!(matches!(
        normalize(&db, root.path(), &cache, &cancel),
        Err(engine_library::Error::Cancelled)
    ));
    assert!(matches!(
        prune(&db, &cache, &cancel),
        Err(engine_library::Error::Cancelled)
    ));
    assert!(cache.join("broken.JPG").exists());
}

#[test]
fn normalizing_a_referenced_cache_image_rekeys_content_hash_and_collection_references() {
    use sha1::Digest;
    let root = support::scratch();
    let cache = root.path().join("Artwork");
    std::fs::create_dir(&cache).unwrap();
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("fixtures/schema.sql"))
        .unwrap();
    let db = Database::from_connection(connection).unwrap();
    let mut rectangular = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut rectangular)
        .encode_image(&DynamicImage::new_rgb8(70, 30))
        .unwrap();
    let old = hex::encode(sha1::Sha1::digest(&rectangular));
    std::fs::write(cache.join(format!("{old}.jpg")), rectangular).unwrap();
    let old_base64 = URL_SAFE_NO_PAD.encode(hex::decode(&old).unwrap());
    std::fs::copy(
        cache.join(format!("{old}.jpg")),
        cache.join(format!("{old_base64}.jpg")),
    )
    .unwrap();
    let mut thumbnail = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut thumbnail)
        .encode_image(&DynamicImage::new_rgb8(256, 256))
        .unwrap();
    let id = db.upsert_art(&old, &thumbnail).unwrap();
    let mut track = Track {
        album_art_id: Some(id),
        album_art_source_hash: Some(old.clone()),
        ..Default::default()
    };
    db.insert_track(&mut track).unwrap();
    let mut only_source = Track {
        album_art_source_hash: Some(old.clone()),
        ..Default::default()
    };
    db.insert_track(&mut only_source).unwrap();
    let report = normalize(&db, root.path(), &cache, &CancellationToken::new()).unwrap();
    assert_eq!(report.cache_normalized_count, 2);
    let updated = db.track(track.id).unwrap().unwrap();
    let hash = updated.album_art_source_hash.unwrap();
    assert_ne!(hash, old);
    let bytes = std::fs::read(cache.join(format!("{hash}.jpg"))).unwrap();
    assert_eq!(hex::encode(sha1::Sha1::digest(&bytes)), hash);
    assert_eq!(
        db.artwork().unwrap()[0].hash.as_deref(),
        Some(hash.as_str())
    );
    assert!(!cache.join(format!("{old}.jpg")).exists());
    assert!(!cache.join(format!("{old_base64}.jpg")).exists());
    assert_eq!(
        db.track(only_source.id)
            .unwrap()
            .unwrap()
            .album_art_source_hash
            .as_deref(),
        Some(hash.as_str())
    );
}
