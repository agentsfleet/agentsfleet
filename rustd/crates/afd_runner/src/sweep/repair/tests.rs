//! When the dispatcher comes back, which is the one thing it decides alone.
//!
//! The claim, the append and the completion are all statements or Redis calls,
//! proven in the integration lane. The PACING is a pure function of what a pass
//! saw, and it is the part most easily got wrong: coming back too soon finds
//! rows this very pass still holds, and coming back too late leaves a backlog
//! draining a batch a minute.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::time::Duration;

use super::{CLAIM_STALE, DUE_BATCH_LIMIT, Dispatched, NOTHING_DUE_INTERVAL};

/// A pass that saw `due` intents, completing `completed` of them.
fn pass(due: usize, completed: usize, failed: usize) -> Dispatched {
    Dispatched {
        due,
        completed,
        failed,
        cleanup_pending: false,
    }
}

/// One full batch, as the claim limit defines it.
fn full() -> usize {
    usize::try_from(DUE_BATCH_LIMIT).expect("the batch limit is positive")
}

#[test]
fn an_empty_pass_waits_the_ordinary_interval() {
    assert_eq!(pass(0, 0, 0).pacing(), NOTHING_DUE_INTERVAL);
}

#[test]
fn a_full_batch_comes_straight_back() {
    // A batch that filled means more intents are already due, and each waits
    // its whole settling window plus this delay before anyone looks again.
    assert_eq!(pass(full(), full(), 0).pacing(), Duration::ZERO);
}

#[test]
fn a_pass_with_failures_waits_exactly_a_claims_life() {
    // The subtle one: a failed intent is still CLAIMED by this pass, and
    // becomes claimable again only once the claim goes stale. Coming back
    // sooner would re-read rows nothing can take yet, and would do it in a
    // tight loop.
    assert_eq!(pass(4, 3, 1).pacing(), CLAIM_STALE);
    // Even on a full batch, if nothing at all completed: something is wrong
    // with every row rather than with one of them.
    assert_eq!(pass(full(), 0, full()).pacing(), CLAIM_STALE);
}

#[test]
fn a_full_batch_that_mostly_worked_still_comes_straight_back() {
    // One failure among a full batch is not a reason to stall the other
    // thirty-one that are waiting: the backlog wins, and the failed row is
    // picked up by whichever pass finds its claim lapsed.
    let mostly = pass(full(), full() - 1, 1);
    assert_eq!(mostly.pacing(), Duration::ZERO);
}

#[test]
fn a_full_cleanup_page_outranks_every_other_pacing() {
    // Keys left in Redis are the one thing that costs money elsewhere, and a
    // full page means more are waiting.
    let pending = Dispatched {
        cleanup_pending: true,
        ..pass(0, 0, 1)
    };
    assert_eq!(pending.pacing(), Duration::ZERO);
}
