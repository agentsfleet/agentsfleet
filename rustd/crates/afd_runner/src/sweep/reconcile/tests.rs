//! What the reconcile sweeper decides without asking anything.
//!
//! The repair itself is statements and a datastore probe, proven in the
//! integration lane against a cluster whose data was really destroyed. What is
//! decidable here is the PACING — a pass that voided nothing waits the ordinary
//! interval, and one that voided rows comes back sooner because its caps mean
//! it probably left more behind — and the caps' relationship to the replay
//! dispatcher that inherits the voided rows.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_admission::Reconciled;

use super::{FLEET_LIMIT, INTERVAL, RECOVERING_INTERVAL, ROW_LIMIT, pacing_after};

/// What one pass concluded, spelled as the fields a reader cares about.
const fn pass(probed: u64, lost: u64, voided: u64) -> Reconciled {
    Reconciled {
        probed,
        lost,
        voided,
        resuming: false,
        declined: 0,
    }
}

/// The same pass, having left a fleet mid-repair.
const fn resuming(probed: u64, lost: u64, voided: u64) -> Reconciled {
    Reconciled {
        resuming: true,
        ..pass(probed, lost, voided)
    }
}

#[test]
fn a_pass_that_found_nothing_waits_the_ordinary_interval() {
    assert_eq!(pacing_after(pass(0, 0, 0)), INTERVAL);
}

#[test]
fn a_pass_that_probed_healthy_fleets_waits_the_ordinary_interval() {
    // The steady state of a busy deployment: every fleet asked about still had
    // its oldest undelivered entry. Probing is not a reason to come back sooner
    // — only repairing is.
    assert_eq!(pacing_after(pass(128, 0, 0)), INTERVAL);
}

#[test]
fn a_pass_that_voided_rows_comes_back_sooner() {
    assert_eq!(pacing_after(pass(4, 1, 1)), RECOVERING_INTERVAL);
}

/// A pass mid-repair comes back sooner even having voided nothing.
///
/// The case the pacing missed before the resume state existed: a walk that
/// probed its whole batch and found every row alive voids zero, and if that
/// read as quiet, the fleet's remaining lost rows would be repaired one batch
/// per IDLE interval — five minutes a batch on work a producer was told yes
/// about.
#[test]
fn a_pass_that_left_a_repair_unfinished_comes_back_sooner() {
    assert_eq!(pacing_after(resuming(4, 1, 0)), RECOVERING_INTERVAL);
}

#[test]
fn a_lost_fleet_that_voided_nothing_waits_the_ordinary_interval() {
    // A fleet whose stream could not answer, whose rows another replica's pass
    // had already repaired: this one probed them, found the receipts it was
    // told about already gone, and its guarded writes matched nothing. This
    // pass repaired nothing, so the work of coming back belongs to whoever did.
    assert_eq!(pacing_after(pass(4, 1, 0)), INTERVAL);
}

#[test]
fn recovery_paces_between_immediate_and_the_steady_state() {
    // Never immediate: the rows a pass voids are the replay dispatcher's
    // backlog, and returning at once would raise that backlog faster than the
    // dispatcher draining it can keep up with.
    assert!(!RECOVERING_INTERVAL.is_zero());
    assert!(RECOVERING_INTERVAL < INTERVAL);
}

#[test]
fn a_lost_fleets_batch_matches_what_the_replay_dispatcher_takes() {
    // Every row this pass voids becomes a row the replay dispatcher re-appends,
    // and that dispatcher takes 32 per pass. A wider batch here would only
    // deepen a backlog it cannot drain any faster.
    assert_eq!(ROW_LIMIT, 32);
    // Each probed fleet is one datastore round trip, so this is the pass's
    // round-trip bound. It is deliberately far wider than the row cap: probing
    // is how a lost fleet is FOUND, and a deployment can hold many more fleets
    // with work in flight than one pass will ever need to repair.
    const { assert!(FLEET_LIMIT > ROW_LIMIT) };
}

/// A reconciler over handles that resolve and never answer.
///
/// Nothing here makes a request: the pass is never run, only its resume state
/// is taken and put back, which is the part that has no datastore in it.
fn reconciler() -> super::Reconcile {
    let environment = afd_core::env::MapEnv::from_pairs([(
        afd_db::config::DbRole::Api.url_knob(),
        "postgres://nowhere/agentsfleet",
    )]);
    let pool = afd_db::config::PoolConfig::resolve(&environment, afd_db::config::DbRole::Api)
        .expect("a lazy pool config resolves");
    let queue = afd_dragonfly::config::DragonflyConfig::from_url(
        afd_dragonfly::config::DragonflyRole::Default,
        "redis://127.0.0.1:1/".to_owned(),
    );
    super::Reconcile::new(afd_admission::Admissions::for_tests(
        afd_db::Db::unreachable(&pool),
        afd_dragonfly::Dragonfly::unreachable(&queue).expect("a well-formed URL builds a handle"),
    ))
}

/// The resume state is taken by value and leaves a usable one behind.
///
/// The shape is the whole point: a synchronous guard held across the pass's
/// `await` would park every other task on this worker thread, so the state
/// comes OUT before the pass and goes back after. What must not happen is the
/// take leaving something unusable behind — a sweeper whose next pass found a
/// set with no capacity would decline every repair forever.
/// Inside a runtime because `sqlx` registers even a LAZY pool with the reactor
/// the moment it is built, and panics when there is none.
#[tokio::test]
async fn taking_the_resume_state_leaves_a_usable_one() {
    let reconcile = reconciler();

    let taken = reconcile.take_progress();
    assert!(
        !taken.is_resuming(),
        "a fresh reconciler carries no unfinished repair"
    );

    let again = reconcile.take_progress();
    assert!(
        !again.is_resuming(),
        "the take left a usable set behind, not a hole"
    );
}
