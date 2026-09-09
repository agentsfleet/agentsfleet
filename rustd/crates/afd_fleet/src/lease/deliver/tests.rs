//! What an ungranted park does, and what it says when it cannot ask.
//!
//! The Failure Mode this table exists for: an ungranted delivery has exactly one
//! terminal outcome and every other reading must leave the work leasable. Ending
//! an event on anything but a person's no throws away work nobody refused, and
//! failing to REPORT an unwritten request hides the loop this milestone exists to
//! end. Both halves are pure functions of the request's answer, so neither needs
//! a datastore.
//!
//! Split from [`super`] on the LENGTH GATE: the module reached 324 lines and
//! these tests would have carried it past the 350 cap.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{Ungranted, answers, written};
use afd_approval::Requested;
use afd_core::id::Uuid7;

#[test]
fn a_denied_grant_is_the_only_outcome_that_ends_the_event() {
    // The failure this milestone exists to end is an event that redelivers
    // every second against a decision nobody can make. A denial IS that
    // decision, so the loop stops here and the operator reads why.
    assert_eq!(answers(Some(Requested::Denied)), Ungranted::Ends);
}

#[test]
fn every_answerable_outcome_leaves_the_delivery_leasable() {
    // A raised card, a card already open, and a grant approved between the
    // assembly's read and this request are three different states and one
    // instruction: wait. The work is not lost, and the next poll runs it.
    for still_open in [Requested::Raised, Requested::Pending, Requested::Approved] {
        assert_eq!(
            answers(Some(still_open)),
            Ungranted::Waits,
            "{still_open:?}"
        );
    }
}

#[test]
fn a_request_that_could_not_be_written_waits_rather_than_ending() {
    // The fail-closed direction, and the one worth a test of its own: a
    // Postgres that would not answer must never be read as a person's no.
    // Ending here would destroy a delivery on an outage, and the outage is
    // the one condition guaranteed to pass.
    assert_eq!(answers(None), Ungranted::Waits);
}

/// The error `written` is handed when an identifier will not encode.
///
/// Built through the same `#[from]` edge production uses — `Uuid7::encode`'s
/// failure lifts into `afd_approval::Error::Identifier` — so this is the real
/// variant with its real `code()`, not a stand-in.
fn unwritable() -> afd_approval::Error {
    afd_approval::Error::from(
        Uuid7::parse("not-a-v7-identifier").expect_err("a malformed id must not parse"),
    )
}

#[test]
fn a_written_request_passes_its_answer_through_untouched() {
    // The reporting wrapper must not become a second decision point: every arm
    // `settle` produced has to arrive at `answers` exactly as it left.
    for answer in [
        Requested::Raised,
        Requested::Pending,
        Requested::Approved,
        Requested::Denied,
    ] {
        assert_eq!(
            written(Ok(answer), &fleet(), "github"),
            Some(answer),
            "{answer:?}"
        );
    }
}

#[test]
fn an_unwritten_request_reports_and_then_reads_as_no_answer() {
    // The bug this catches is the one the milestone is about, reproduced on its
    // own fix: `.ok()` dropped this error, so a Postgres or entropy failure left
    // no line, no error_code, and a `no_work` identical to a healthy park — a
    // failure with no error, redelivering every second. `written` must answer
    // `None` (so the delivery still waits) AND emit the line that says why.
    assert_eq!(written(Err(unwritable()), &fleet(), "github"), None);
}

#[test]
fn an_unwritten_request_never_ends_the_event() {
    // The composition that matters. Fail-closed here means keeping the event
    // alive: a datastore that would not answer is this instance's problem, and
    // reading its silence as a refusal would end deliveries on an outage — the
    // one condition guaranteed to occur.
    assert_eq!(
        answers(written(Err(unwritable()), &fleet(), "github")),
        Ungranted::Waits
    );
}

/// A canonical v7 identifier, which `written` reads only for its log field.
fn fleet() -> Uuid7 {
    Uuid7::parse("01890a5d-ac96-774b-bcce-b302099a8057").expect("a valid v7 identifier")
}
