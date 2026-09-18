//! Rotation, wrap, resume and the memory bound, with no datastore in sight.
//!
//! The pass these serve needs two live datastores to grade, and the integration
//! suite grades it there. What is decidable here is the STATE MACHINE: which
//! fleet the next scan starts after, when it starts over, which fleets are
//! resumed and from where, and what happens when more fleets want remembering
//! than the bound allows. Those are the branches the two starvation failures
//! lived in, and a failure that reproduces in a hundred microseconds is one a
//! reader can run while changing the logic.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and a resume list of a known length \
              indexed out of range should say so rather than quietly pass a \
              None along; the restriction set is for the daemon"
)]

use super::{FIRST_FLEET, Progress, Repair, RowKey};

/// One pass's fleet budget, where a test drains repairs rather than files them.
const BUDGET: i64 = 4;

/// The resume-set capacity these tests bound, small enough to fill on purpose.
const CAPACITY: usize = 4;

/// A row key that is not the first, for the resume assertions.
const SOMEWHERE: RowKey = RowKey {
    created_at: 1_750_000_000_000,
    seq: 42,
};

/// A fresh reconciler starts at the bottom of the fleet ordering.
///
/// The bound is always present in the statement, so "no cursor yet" has to be a
/// value rather than an absence, and this is the value.
#[test]
fn a_first_pass_starts_below_every_fleet() {
    let progress = Progress::with_capacity(CAPACITY);
    assert_eq!(progress.resume_from(), FIRST_FLEET);
    assert!(!progress.is_resuming(), "nothing is mid-repair yet");
}

/// A pass that filled its budget resumes after the last fleet it read.
///
/// This is the whole of the fleet-starvation repair: without it the next scan
/// reads the same lowest-sorting fleets, and a deployment with more unfinished
/// fleets than one pass examines never reaches the rest.
#[test]
fn a_full_sweep_resumes_after_its_last_fleet() {
    let mut progress = Progress::with_capacity(CAPACITY);
    progress.swept(Some("fleet-c".to_owned()), true);
    assert_eq!(progress.resume_from(), "fleet-c");
}

/// A pass that did not fill its budget starts over.
///
/// The other half: the rotation has to WRAP, or the cursor walks off the end of
/// the ordering and a fleet sorting below it is stranded exactly as surely as
/// one sorting above it was before.
#[test]
fn a_short_sweep_wraps_to_the_start() {
    let mut progress = Progress::with_capacity(CAPACITY);
    progress.swept(Some("fleet-c".to_owned()), true);
    progress.swept(Some("fleet-e".to_owned()), false);
    assert_eq!(progress.resume_from(), FIRST_FLEET);
}

/// An empty sweep wraps too, rather than holding a cursor nothing produced.
#[test]
fn a_sweep_that_read_nothing_wraps() {
    let mut progress = Progress::with_capacity(CAPACITY);
    progress.swept(Some("fleet-c".to_owned()), true);
    progress.swept(None, false);
    assert_eq!(progress.resume_from(), FIRST_FLEET);
}

/// Rotating over more fleets than one pass examines reaches every one of them.
///
/// The counterexample in full, as a loop: five fleets, a budget of four. The
/// failing design visits `a b c d` forever and never `e`. This asserts the
/// whole set is covered, and that the pass after the wrap starts over rather
/// than stopping.
#[test]
fn rotation_visits_every_unfinished_fleet() {
    let fleets = ["fleet-a", "fleet-b", "fleet-c", "fleet-d", "fleet-e"];
    let mut progress = Progress::with_capacity(CAPACITY);
    let mut seen: Vec<&str> = Vec::new();

    for _pass in 0..3 {
        let after = progress.resume_from().to_owned();
        let slice: Vec<&str> = fleets
            .iter()
            .copied()
            .filter(|fleet| *fleet > after.as_str())
            .take(usize::try_from(BUDGET).expect("a budget fits a usize"))
            .collect();
        seen.extend(slice.iter().copied());
        let filled = i64::try_from(slice.len()).is_ok_and(|read| read >= BUDGET);
        progress.swept(slice.last().map(|last| (*last).to_owned()), filled);
    }

    for fleet in fleets {
        assert!(seen.contains(&fleet), "{fleet} was never examined");
    }
}

/// A walk that reached the end of a fleet's rows files no resume point.
#[test]
fn a_short_walk_retires_its_fleet() {
    let mut progress = Progress::with_capacity(CAPACITY);
    assert!(progress.walked("fleet-a", None));
    assert!(!progress.is_resuming(), "nothing left to carry on");
    assert!(progress.resume_repairs(BUDGET).is_empty());
}

/// A walk that filled its batch is resumed from where it stopped.
///
/// The row-starvation repair. The next pass must not re-read this fleet's
/// oldest rows: recovery has just made them healthy, and a walk that starts
/// there fills its batch with rows it already fixed and never reaches the ones
/// it did not.
#[test]
fn a_full_walk_resumes_where_it_stopped() {
    let mut progress = Progress::with_capacity(CAPACITY);
    assert!(progress.walked("fleet-a", Some(SOMEWHERE)));
    assert!(progress.is_resuming());

    let resumed = progress.resume_repairs(BUDGET);
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].fleet_id, "fleet-a");
    assert_eq!(resumed[0].after, SOMEWHERE);
}

/// Resuming a fleet takes it off the list, so one pass cannot walk it twice.
#[test]
fn a_resumed_fleet_is_taken_off_the_list() {
    let mut progress = Progress::with_capacity(CAPACITY);
    progress.walked("fleet-a", Some(SOMEWHERE));
    assert_eq!(progress.resume_repairs(BUDGET).len(), 1);
    assert!(
        progress.resume_repairs(BUDGET).is_empty(),
        "the fleet is being walked; it is not also queued"
    );
    assert!(!progress.is_resuming());
}

/// The resume set never grows past its declared capacity.
///
/// The memory bound, and the one place this design trades coverage SPEED for
/// it: the declined fleet is still reachable through the head probe, which is
/// why declining is safe and why the caller logs it. A capacity of its own
/// rather than the pass's fleet budget — those are a memory bound and a
/// round-trip budget, and tying them made one silently move the other.
#[test]
fn the_resume_set_respects_its_capacity() {
    let mut progress = Progress::with_capacity(CAPACITY);

    for fleet in 0..CAPACITY {
        assert!(
            progress.walked(&format!("fleet-{fleet}"), Some(SOMEWHERE)),
            "fleet {fleet} fits inside the capacity"
        );
    }
    assert!(
        !progress.walked("one-too-many", Some(SOMEWHERE)),
        "a full set declines rather than evicting a fleet mid-repair"
    );
    assert_eq!(progress.resume_repairs(BUDGET).len(), CAPACITY);
}

/// Resuming takes at most one pass's worth, oldest first.
///
/// A pass cannot spend its whole round-trip budget draining a backlog of
/// repairs and then have nothing left for the rotation — which would stall the
/// fleet cursor for as long as the repairs lasted.
#[test]
fn resuming_takes_at_most_one_pass_worth() {
    let mut progress = Progress::with_capacity(CAPACITY);
    for fleet in 0..4 {
        progress.walked(&format!("fleet-{fleet}"), Some(SOMEWHERE));
    }

    let resumed = progress.resume_repairs(2);
    assert_eq!(resumed.len(), 2);
    assert_eq!(resumed[0].fleet_id, "fleet-0", "oldest repair first");
    assert!(progress.is_resuming(), "the rest are still queued");
    assert_eq!(progress.resume_repairs(BUDGET).len(), 2);
}

/// Every admission sorts above the first row key.
///
/// Decided at compile time rather than in a test, for the reason
/// `sweep/reconcile.rs` gives for its own bounds: a runtime check over two
/// constants reports a bad edit as a red suite instead of as a build that does
/// not produce a binary. Pinned at all because the statement binds this value
/// as a real lower bound rather than as a stand-in for absence — `created_at`
/// is a Unix millisecond and `seq` an identity column from one, so a key at or
/// below zero is not a row this daemon wrote.
const _: () = {
    assert!(
        RowKey::FIRST.created_at == 0 && RowKey::FIRST.seq == 0,
        "the resume floor is the bottom of both columns"
    );
    assert!(
        SOMEWHERE.created_at > RowKey::FIRST.created_at,
        "a real row sorts above the floor a first walk binds"
    );
};

/// A repair put back after a failed walk keeps its place in the queue.
///
/// `resume_repairs` DRAINS, so a pass that fails partway is holding resume
/// points that exist nowhere else. Dropping them would return those fleets to
/// the head probe, which after a partial repair is exactly the shortcut that
/// cannot see their remaining lost rows — a database blip would cost the
/// coverage this whole structure exists to give.
#[test]
fn a_refiled_repair_is_walked_again() {
    let mut progress = Progress::with_capacity(CAPACITY);
    progress.walked("fleet-a", Some(SOMEWHERE));
    progress.walked("fleet-b", Some(SOMEWHERE));

    let drained = progress.resume_repairs(BUDGET);
    assert_eq!(drained.len(), 2);
    assert!(!progress.is_resuming(), "draining empties the set");

    for repair in drained {
        assert!(
            progress.refile(repair),
            "the set has room for what it drained"
        );
    }

    let again = progress.resume_repairs(BUDGET);
    assert_eq!(again.len(), 2, "both fleets are walked again");
    assert_eq!(
        again[0].fleet_id, "fleet-a",
        "and in the order they were filed"
    );
    assert_eq!(
        again[1].after, SOMEWHERE,
        "carrying the row key they stopped at"
    );
}

/// Re-filing into a full set declines, the same as any other way in.
#[test]
fn a_refile_respects_the_capacity() {
    let mut progress = Progress::with_capacity(CAPACITY);
    let spare = Repair {
        fleet_id: "fleet-spare".to_owned(),
        after: SOMEWHERE,
    };
    for fleet in 0..CAPACITY {
        progress.walked(&format!("fleet-{fleet}"), Some(SOMEWHERE));
    }

    assert!(
        !progress.refile(spare),
        "a full set turns a re-file away rather than growing past its bound"
    );
}

/// A cursor naming a fleet that no longer exists resumes at the next one.
///
/// The bound is a strict inequality on a VALUE, not a reference to a row, so a
/// fleet deleted between two passes needs no handling at all — the scan simply
/// begins at whatever sorts after where it was. Pinned because the obvious
/// alternative, storing a row identifier and looking it up, would stall the
/// rotation on exactly this case: a cursor pointing at nothing, and a scan with
/// nowhere to resume from.
#[test]
fn a_cursor_survives_a_deleted_fleet() {
    let mut progress = Progress::with_capacity(CAPACITY);
    progress.swept(Some("fleet-c".to_owned()), true);

    // `fleet-c` is gone; the deployment now holds only these.
    let remaining = ["fleet-a", "fleet-b", "fleet-d", "fleet-e"];
    let after = progress.resume_from().to_owned();
    let next: Vec<&str> = remaining
        .iter()
        .copied()
        .filter(|fleet| *fleet > after.as_str())
        .collect();

    assert_eq!(
        next,
        vec!["fleet-d", "fleet-e"],
        "the scan carries on after where the deleted fleet sorted, not from the start"
    );
}
