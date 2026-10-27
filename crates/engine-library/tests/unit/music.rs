#[path = "../support/mod.rs"]
mod support;
use super::*;

#[test]
fn staged_move_reports_committed_and_rollback_cleanup_failures_without_losing_primary_error() {
    let root = support::scratch();
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::write(&source, b"audio").unwrap();
    let token = CancellationToken::new();
    let movement = FileMove::prepare(&source, &target, &token).unwrap();
    fs::remove_file(&source).unwrap();
    assert!(matches!(
        movement.commit(),
        Err(Error::CommittedCleanup { .. })
    ));
    assert_eq!(fs::read(&target).unwrap(), b"audio");
    fs::write(&source, b"original").unwrap();
    fs::remove_file(&target).unwrap();
    let movement = FileMove::prepare(&source, &target, &token).unwrap();
    fs::remove_file(&target).unwrap();
    let error = movement.rollback(Error::Integrity("database failed".into()));
    let Error::Cleanup { primary, path, .. } = error else {
        panic!("expected cleanup error");
    };
    assert!(matches!(*primary, Error::Integrity(_)));
    assert_eq!(path, target);
    assert_eq!(fs::read(&source).unwrap(), b"original");
    assert!(byte_count(u64::MAX).is_err());
}
