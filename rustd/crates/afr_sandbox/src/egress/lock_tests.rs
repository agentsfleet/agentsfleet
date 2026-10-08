#![expect(
    clippy::unwrap_used,
    reason = "test module: a precondition that fails should fail the test loudly"
)]

use super::take;

/// A second holder is refused while the first holds the lock, and saying so
/// names the reason; once the first lets go, the lock is free again. Two opens
/// of one file stand in for two processes: `flock` belongs to the open file.
#[test]
fn test_a_second_holder_is_refused_until_the_first_lets_go() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run").join("egress.lock");

    let first = take(&path).unwrap();
    let refused = take(&path).unwrap_err();
    drop(first);
    let again = take(&path);

    assert!(
        refused.to_string().contains("another runner process"),
        "{refused}"
    );
    again.unwrap();
}

/// A lock file whose directory cannot be made is an error, never a lock.
#[test]
fn test_an_unopenable_lock_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, "").unwrap();

    take(&blocker.join("egress.lock")).unwrap_err();
}
