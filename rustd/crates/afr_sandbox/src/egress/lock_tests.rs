#![expect(
    clippy::unwrap_used,
    reason = "test module: a precondition that fails should fail the test loudly"
)]

use std::fs::TryLockError;
use std::io;

use super::{refused, take};
use crate::error::EgressRefusal;

/// A second holder is refused while the first holds the lock, and saying so
/// names the reason; once the first lets go, the lock is free again. Two opens
/// of one file stand in for two processes: `flock` belongs to the open file.
#[test]
fn test_a_second_holder_is_refused_until_the_first_lets_go() {
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("run");

    let first = take(&run).unwrap();
    let refused = take(&run).unwrap_err();
    drop(first);
    let again = take(&run);

    assert_eq!(
        refused.egress_refusal(),
        Some(&EgressRefusal::HeldElsewhere)
    );
    again.unwrap();
}

/// A lock file whose directory cannot be made is an error, never a lock.
#[test]
fn test_an_unopenable_lock_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, "").unwrap();

    take(&blocker).unwrap_err();
}

/// A lock call that fails for any reason but another holder is that failure,
/// its system cause kept, and never a refusal naming a holder that may not
/// exist.
#[test]
fn test_a_failed_lock_call_is_its_own_error() {
    let failed = refused(TryLockError::Error(io::Error::from_raw_os_error(
        libc::ENOLCK,
    )));
    let held = refused(TryLockError::WouldBlock);

    let cause = std::error::Error::source(&failed)
        .and_then(|source| source.downcast_ref::<io::Error>())
        .and_then(io::Error::raw_os_error);
    assert_eq!(failed.egress_refusal(), None);
    assert_eq!(cause, Some(libc::ENOLCK));
    assert_eq!(held.egress_refusal(), Some(&EgressRefusal::HeldElsewhere));
}
