//! Reconciling the store against a REAL external scheduler.
//!
//! # Why this suite exists and the others could not replace it
//!
//! `integration_fence.rs` proves the fence and `error_surface.rs` proves the
//! typing of a refusal, but both stop at the seam: neither ever speaks to a
//! scheduler. Every question that only a real vendor can answer — does the
//! configured base actually get dialled, does the credential authenticate, is
//! the key it files the schedule under the key a later delete has to name —
//! lives here.
//!
//! # The scheduler is real, and it is local
//!
//! `docker-compose.yml`'s `qstash` service runs Upstash's own dev server with
//! deterministic credentials, and `make/test-infra.mk` starts it beside Postgres
//! and Redis. This suite reads the two knobs that lane exports and SELF-SKIPS
//! when they are unset, so `cargo test` outside the lane stays green without
//! silently pretending to have proved any of this.
//!
//! # The bug this suite was written against
//!
//! A create invents `source_key` before it has ever spoken to the scheduler
//! (`{fleet_id}-{millis}`), and the scheduler files the schedule under an id of
//! its own. `reconcile` used to discard that id. Since `remove` treats a 404 as
//! success — correctly, because a schedule already gone has met the caller's
//! goal — a pause or delete then named a key the scheduler never issued, got a
//! 404, and reported success while the schedule kept firing forever. Nothing
//! that talks to a fake catches this: a fake answers whatever key it is asked
//! about. `the_scheduler_key_is_adopted_and_a_delete_names_it` is the guard.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_cron::qstash::QStash;
use afd_cron::{DesiredStatus, Reconciled, ScheduleService as Reconciler, SyncStatus};

use self::support::CronLane;

/// Where the lane's scheduler listens.
const LIVE_URL_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_URL";

/// The credential it authenticates that lane against.
const LIVE_TOKEN_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_TOKEN";

/// A destination the dev server will accept.
///
/// It resolves the destination for real at create time, so a `.test` host is
/// refused before any of this suite's actual subject is reached.
const LIVE_DESTINATION: &str = "https://example.com";

/// A base no scheduler listens on, for the outage half.
///
/// `.test` is reserved and resolves nowhere, so this fails as a TRANSPORT
/// failure — the case Dimension 3.3 is about — rather than as a refusal from
/// something that answered.
const UNREACHABLE_BASE: &str = "https://qstash.unreachable.test/v2";

/// The scheduler this lane talks to, or `None` outside the lane.
///
/// Says so on the way out. A silent `return` reports `ok` in the same words a
/// real pass does, so a lane that stopped exporting these knobs would go on
/// reporting four passes over two tests that never ran — which is how this
/// suite's own base URL stayed wrong long enough to be found by accident.
fn live() -> Option<(String, String)> {
    let url = std::env::var(LIVE_URL_KNOB).ok().filter(|v| !v.is_empty());
    let token = std::env::var(LIVE_TOKEN_KNOB)
        .ok()
        .filter(|v| !v.is_empty());
    match (url, token) {
        (Some(url), Some(token)) => Some((url, token)),
        _unset => {
            eprintln!(
                "SKIPPED: no live scheduler — {LIVE_URL_KNOB} and {LIVE_TOKEN_KNOB} are what \
                 `make test-integration-rustd` exports"
            );
            None
        }
    }
}

/// A reconciler bound to the real scheduler.
fn against_live(lane: &CronLane, url: String, token: String) -> Reconciler {
    Reconciler::new(
        lane.store.clone(),
        QStash::new(
            reqwest::Client::new(),
            token,
            LIVE_DESTINATION.to_owned(),
            url,
        ),
    )
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_real_scheduler_accepts_the_push_and_the_row_records_it() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let created = lane.settled("key-live-push", "0 4 * * *").await;
    let claim = CronLane::token();
    let held = lane
        .store
        .claim_current(
            &lane.fleet_id(),
            &created.schedule_id,
            &claim,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");

    let reconciled = against_live(&lane, url, token)
        .reconcile(&held, &claim, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure");

    let Reconciled::Synced(row) = reconciled else {
        panic!("a scheduler that accepted the push must leave the row synced, got {reconciled:?}");
    };
    assert_eq!(row.sync_status, SyncStatus::Synced);
    assert!(
        row.last_error.is_none(),
        "a push that succeeded leaves no reason behind"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn the_scheduler_key_is_adopted_and_a_delete_names_it() {
    // The whole point of the suite. A create invents `source_key` locally; the
    // scheduler files the schedule under its own id. Unless the push ADOPTS
    // that id, every later call names a key the vendor never issued.
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let invented = "key-live-adopt";
    let created = lane.settled(invented, "0 5 * * *").await;
    assert_eq!(
        created.source_key, invented,
        "a create stores the placeholder it invented, having spoken to nobody"
    );

    let claim = CronLane::token();
    let held = lane
        .store
        .claim_current(
            &lane.fleet_id(),
            &created.schedule_id,
            &claim,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");

    let service = against_live(&lane, url, token);
    let reconciled = service
        .reconcile(&held, &claim, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure");

    let Reconciled::Synced(synced) = reconciled else {
        panic!("the push must have been accepted, got {reconciled:?}");
    };
    assert_ne!(
        synced.source_key, invented,
        "the row must carry the scheduler's own key, not the one the create invented"
    );
    assert!(
        !synced.source_key.is_empty(),
        "the adopted key is what a later delete names"
    );

    // And the adopted key is usable: removing under it is accepted by the
    // scheduler that issued it. Asserted rather than swallowed, because
    // `remove` reports a 404 as success — the exact reason a wrong key hides.
    let deleting = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &synced.schedule_id,
            afd_cron::Change {
                cron: None,
                timezone: None,
                message: None,
                desired_status: Some(DesiredStatus::Deleting),
            },
            &claim,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the schedule is claimable for deletion");

    let removed = service
        .reconcile(&deleting, &claim, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure");
    assert!(
        matches!(removed, Reconciled::Removed),
        "a delete the scheduler agreed with removes the row, got {removed:?}"
    );
    assert_eq!(
        lane.count().await,
        0,
        "the row goes only once the scheduler has agreed"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_scheduler_that_cannot_be_reached_keeps_its_reason_on_the_row() {
    // Dimension 3.3's half that needs no vendor: the store and the upstream
    // diverge, and the row says so in a way the next `:sync` can act on.
    let lane = CronLane::open().await;
    let created = lane.settled("key-live-outage", "0 6 * * *").await;
    let claim = CronLane::token();
    let held = lane
        .store
        .claim_current(
            &lane.fleet_id(),
            &created.schedule_id,
            &claim,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");

    let stalled = Reconciler::new(
        lane.store.clone(),
        QStash::new(
            reqwest::Client::new(),
            "unused".to_owned(),
            LIVE_DESTINATION.to_owned(),
            UNREACHABLE_BASE.to_owned(),
        ),
    );
    let reconciled = stalled
        .reconcile(&held, &claim, CronLane::now())
        .await
        .expect("an unreachable scheduler is recorded, never raised");

    let Reconciled::Failed(row) = reconciled else {
        panic!("an unreachable scheduler leaves the row failed, got {reconciled:?}");
    };
    assert_eq!(row.sync_status, SyncStatus::Failed);
    let reason = row
        .last_error
        .as_deref()
        .expect("a failed push keeps why on the row");
    assert!(
        !reason.is_empty(),
        "the reason is what the operator and the next sync both read"
    );
    assert!(
        !reason.contains(UNREACHABLE_BASE),
        "the stored sentence is this daemon's own, never the transport's target"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_push_under_a_lost_fence_changes_nothing() {
    // The store half of 3.1 that a single-syncer test cannot reach: a syncer
    // whose fence was taken while it was talking to a REAL scheduler must not
    // write back, even though its push genuinely succeeded upstream.
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let created = lane.settled("key-live-superseded", "0 7 * * *").await;
    let loser = CronLane::token();
    let held = lane
        .store
        .claim_current(
            &lane.fleet_id(),
            &created.schedule_id,
            &loser,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");

    // A second syncer takes it while the first is mid-push.
    lane.expire_lease(&held.schedule_id).await;
    let winner = CronLane::token();
    lane.store
        .claim_current(
            &lane.fleet_id(),
            &held.schedule_id,
            &winner,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("an expired lease is reclaimable");

    let reconciled = against_live(&lane, url, token)
        .reconcile(&held, &loser, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure");
    assert!(
        matches!(reconciled, Reconciled::Superseded),
        "a syncer that lost its fence writes nothing back, got {reconciled:?}"
    );
}
