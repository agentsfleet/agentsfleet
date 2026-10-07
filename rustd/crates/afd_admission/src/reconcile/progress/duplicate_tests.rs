//! A fleet reported by both halves of one pass keeps a single place in the
//! resume set, so a duplicate never costs another fleet its repair.

#![expect(
    clippy::indexing_slicing,
    reason = "a resume list of a known length indexed out of range should say \
              so rather than quietly pass a None along; the restriction set is \
              for the daemon"
)]

use super::tests::{BUDGET, CAPACITY, SOMEWHERE};
use super::{Progress, Repair, RowKey};

/// One fleet holds one place in the set, however many walks report it.
///
/// Both halves of a pass can report the same fleet. `continue_repairs` drains a
/// mid-repair fleet, walks it, and files the point it stopped at; `sweep_heads`
/// then runs, and that fleet is still receipted and undelivered, so the scan can
/// hand it back and its head probe can answer LOST — the rows the first half
/// voided are no longer receipted, so the oldest RECEIPTED undelivered row is a
/// different one, and it can be dead. The head walk then reports the same fleet
/// a second time.
///
/// Two entries for one fleet spend two of a bounded set's places, walk the fleet
/// twice per pass, and decline some other fleet's repair for the room — and a
/// declined repair is the one case this design gives up reachability, not speed.
///
/// The surviving entry is the EARLIER of the two. Resuming lower re-probes rows
/// that are already healthy, which costs round trips; resuming higher steps over
/// rows nothing has asked about, which is the starvation this milestone exists
/// to remove.
#[test]
fn a_fleet_holds_one_place_however_many_walks_report_it() {
    /// Where the continue-walk stopped: further into the fleet's rows.
    const FURTHER: RowKey = RowKey {
        created_at: 1_750_000_000_000,
        seq: 90,
    };
    /// Where the head walk stopped, having started at the first row.
    const EARLIER: RowKey = RowKey {
        created_at: 1_600_000_000_000,
        seq: 7,
    };

    let mut progress = Progress::with_capacity(CAPACITY);
    assert!(
        progress.walked("fleet-a", Some(FURTHER)),
        "the continue-walk files its point"
    );
    assert!(
        progress.walked("fleet-a", Some(EARLIER)),
        "the head walk reports the same fleet"
    );

    let queued = progress.resume_repairs(BUDGET);
    assert_eq!(
        queued,
        vec![Repair {
            fleet_id: "fleet-a".to_owned(),
            after: EARLIER
        }],
        "one place, resumed from the lower of the two points"
    );
}

/// A duplicate report does not cost another fleet its place in the set.
///
/// The consequence that makes the duplicate worth removing rather than
/// tolerating: with the set full of one fleet reported twice, a genuinely
/// different fleet is declined, and declining is where coverage degrades.
#[test]
fn a_repeated_fleet_does_not_crowd_out_a_different_one() {
    let mut progress = Progress::with_capacity(2);
    assert!(progress.walked("fleet-a", Some(SOMEWHERE)));
    assert!(progress.walked(
        "fleet-a",
        Some(RowKey {
            created_at: 1,
            seq: 1
        })
    ));
    assert!(
        progress.walked("fleet-b", Some(SOMEWHERE)),
        "fleet-b takes the second place, which fleet-a was not entitled to twice"
    );

    let queued = progress.resume_repairs(BUDGET);
    assert_eq!(queued.len(), 2, "two fleets, two places");
    assert_eq!(queued[0].fleet_id, "fleet-a");
    assert_eq!(queued[1].fleet_id, "fleet-b");
}
