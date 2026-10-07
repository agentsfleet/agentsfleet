//! The registry's book, read directly: an end recorded for an id it never
//! started.

use super::registry::{Kind, Registry, Status};

/// An id the registry never started.
const NEVER_STARTED: u64 = 7;

/// Ending an id with no child answers the status given, and keeps no child
/// for it: there is no earlier end to keep, and nothing to list.
#[test]
fn ending_an_id_with_no_child_answers_the_status_given() {
    let (registry, _asked) = Registry::new();

    let done = registry.ended(NEVER_STARTED, Status::Done("an answer".to_owned()));
    let interrupted = registry.ended(NEVER_STARTED, Status::Interrupted);

    assert_eq!(done, Kind::Done);
    assert_eq!(interrupted, Kind::Interrupted, "no earlier end was kept");
    assert!(registry.list().is_empty());
    assert_eq!(registry.calls(NEVER_STARTED), 0);
}
