//! A symlink in a manifest page may climb only as far as its own directory depth.

use super::link;
use crate::{durable_task::DurableTaskRefusal, error::ErrorKind};

fn climbs(path: &str, target: &str) -> Result<(), Option<DurableTaskRefusal>> {
    link(path, target).map_err(|error| match error.kind() {
        ErrorKind::DurableTaskRefused { reason } => Some(*reason),
        _ => None,
    })
}

// Value: protects=an empty or `.` segment never buys a link an extra level to climb
// out of the checkpoint root; fails_when=the `"" | "."` arm counts as a descent;
// why_new=no case had a link with an empty or current-directory segment; seam=the
// page validator's link check.
#[test]
fn current_and_empty_segments_neither_descend_nor_climb() {
    assert_eq!(climbs("a/link", "./../b"), Ok(()), "one level up from a/");
    assert_eq!(
        climbs("a/link", ".//../b"),
        Ok(()),
        "a doubled slash is no level"
    );
    assert_eq!(
        climbs("a/link", "./../../b"),
        Err(Some(DurableTaskRefusal::CheckpointIncompatible)),
        "`.` does not pay for the second climb"
    );
    assert_eq!(
        climbs("link", ".//./../b"),
        Err(Some(DurableTaskRefusal::CheckpointIncompatible)),
        "neither do empty segments"
    );
    assert_eq!(
        climbs("link", "./b/../c"),
        Ok(()),
        "a named segment does descend"
    );
}
