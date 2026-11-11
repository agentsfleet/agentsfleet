//! One issued, active lease with a funded tenant behind it.
//!
//! Builds on `fleet_lease_seed` through the claim, narrative log and lease row,
//! so report tests share the same funded starting state.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::LazyLock;

use af_core::id::Uuid7;
use afd_billing::RUN_NANOS_PER_SEC;
use afd_billing::nanos::MS_PER_SEC;
use afd_fleet::lease::usage::Pricing;
use afd_fleet::lease::{Billed, Delivery, Fence, Issued, Leases};
use jiff::Timestamp;

use crate::rows::Row;
use crate::seed::{ACTOR, MODEL, POSTURE, PROVIDER, REQUEST_JSON, Seeded, seeded_for};
use crate::support::Fixtures;

/// A pool deep enough that nothing under test clamps against it.
///
/// Dimension 3.1 is about row SHAPE, and a wallet that ran dry mid-assertion
/// would clamp `charged` and make the ledger disagree with the slice for a
/// reason that has nothing to do with parity.
pub(crate) const DEEP_POOL: i64 = 1_000_000_000_000;

/// The slice every §3 settle assertion prices: one second of runtime.
///
/// Named because three different facts below are the same number and are NOT
/// the same thing — this duration, the rate that prices it, and the nanos it
/// costs. Spelled as literals they would look like one value repeated, which is
/// exactly the reading RULE UFS exists to prevent.
pub(crate) const SLICE_MS: i64 = 1_000;

/// [`SLICE_MS`] as a span, for the instants a suite reports at.
pub(crate) const SLICE: std::time::Duration =
    std::time::Duration::from_millis(SLICE_MS.unsigned_abs());

/// What one [`SLICE_MS`] slice costs at the run fee every lease pays.
///
/// Derived rather than written so the assertions read as the arithmetic they
/// are checking; [`run_fee`] rounds a partial nano up, as the rater does.
pub(crate) const SLICE_NANOS: i64 = SLICE_MS * RUN_NANOS_PER_SEC / MS_PER_SEC;

/// [`RESUME_EVENT_ID`](crate::report_commit::RESUME_EVENT_ID) as the
/// identifier a terminal report carries, parsed once.
pub(crate) fn resume_event() -> &'static Uuid7 {
    static RESUME: LazyLock<Uuid7> = LazyLock::new(|| {
        Uuid7::parse(crate::report_commit::RESUME_EVENT_ID)
            .expect("the resume event id is a canonical UUIDv7")
    });
    &RESUME
}

/// One issued, active lease with a funded tenant behind it.
///
/// Every test here starts from the same precondition — §2 has already run —
/// and assembling it inline four times is four chances for the tests to
/// disagree about what "an issued lease" means.
pub(crate) struct Held {
    pub(crate) acquired: afd_fleet::lease::Acquired,
    pub(crate) fixtures: Fixtures,
    /// The issued lease as the work its usage lines name.
    pub(crate) lease_key: Uuid7,
    /// What the lease's slices are priced against.
    pub(crate) pricing: Pricing,
    pub(crate) leases: Leases,
    pub(crate) runner: Uuid7,
    pub(crate) spare: Uuid7,
    pub(crate) issued: Issued,
    /// The token the issued lease carries — what a later reclaim must outrank
    /// for Dimension 3.2 to be testing anything.
    pub(crate) fence: Fence,
    pub(crate) fleet: Uuid7,
    pub(crate) tenant: Uuid7,
    pub(crate) event_id: Uuid7,
    /// When the lease was issued, by the database clock.
    pub(crate) now: Timestamp,
}

/// Runs §2 far enough to leave one active lease on one funded fleet.
pub(crate) async fn held() -> Held {
    held_in(Fixtures::create().await).await
}

/// [`held`], over `fixtures`: a private database for a test that breaks a
/// table every other test reads.
pub(crate) async fn held_in(fixtures: Fixtures) -> Held {
    held_admission(fixtures, ACTOR, REQUEST_JSON).await
}

/// One funded native lease over this scenario's admitted request and actor.
pub(crate) async fn held_admission(fixtures: Fixtures, actor: &str, request_json: &str) -> Held {
    let Seeded {
        runners: [runner, spare],
        fleet,
        tenant,
        ..
    } = seeded_for::<2>(&fixtures, actor, request_json).await;
    let leases = fixtures.leases();
    let now = fixtures.enrolled_at();
    fixtures.seed_wallet(&tenant, DEEP_POOL).await;

    let acquired = crate::seed::select_fleet(&leases, &runner, &fleet)
        .await
        .expect("the seeded fleet is leasable");
    assert_eq!(
        acquired.opened.received.delivery,
        Delivery::First,
        "a newly leased event has no row yet"
    );
    let issued = issue_received(&leases, &runner, &acquired, &tenant, now).await;
    let event_id = acquired.event_id.clone();
    let lease_key = issued.lease_id.clone();
    let pricing = fixtures.pricing(&issued.lease_id).await;
    // The issue instant, which issue stamps from the database clock; a slice
    // the suites report is measured from it, as the report's charge is.
    let now = fixtures.metered_since(&fleet).await;

    Held {
        fixtures,
        lease_key,
        pricing,
        leases,
        runner,
        spare,
        issued,
        fence: acquired.fence,
        acquired,
        fleet,
        tenant,
        event_id,
        now,
    }
}

async fn issue_received(
    leases: &Leases,
    runner: &Uuid7,
    acquired: &afd_fleet::lease::Acquired,
    tenant: &Uuid7,
    now: Timestamp,
) -> Issued {
    leases
        .issue(
            runner,
            acquired,
            Billed {
                tenant_id: tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written")
        .expect("the claim is still held")
}

/// Another fleet's lease, held by the same runner as a [`Held`].
pub(crate) struct Other {
    pub(crate) lease: Uuid7,
    pub(crate) fleet: Uuid7,
    pub(crate) tenant: Uuid7,
    pub(crate) workspace: Uuid7,
}

/// Room for every lease a test hands one host.
const HOST_WORKERS: i32 = 8;

impl Held {
    /// The target a gateway session holds for this lease, read from the lease
    /// row as a test has aged it: whether the lease is still the live holder
    /// decides whether the frames it forwards count toward delivery timing.
    pub(crate) async fn activity_target(&self) -> afd_fleet::lease::activity::Target {
        let (created_at, event_created_at, live): (
            jiff_sqlx::Timestamp,
            jiff_sqlx::Timestamp,
            bool,
        ) = sqlx::query_as(
            "SELECT l.created_at, l.event_created_at,
                    l.status = $3 AND l.lease_expires_at > statement_timestamp()
                        AND EXISTS (SELECT 1 FROM runner.fleet_slots s
                                    WHERE s.fleet_id = l.fleet_id
                                      AND s.last_runner_id = l.runner_id
                                      AND s.fencing_token = l.fencing_token
                                      AND s.lease_expires_at > statement_timestamp())
             FROM runner.leases l WHERE l.id = $1 AND l.runner_id = $2",
        )
        .bind(&self.issued.lease_id)
        .bind(&self.runner)
        .bind(afd_state::sql::LEASE_STATUS_ACTIVE)
        .fetch_one(
            &mut *self
                .fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the held lease reads");
        afd_fleet::lease::activity::Target {
            fleet_id: self.fleet.clone(),
            event_id: self.event_id.clone(),
            lease_created_at: created_at.to_jiff(),
            event_created_at: event_created_at.to_jiff(),
            timing_eligible: live,
            fence: self.fence.as_i64(),
        }
    }

    /// Another fleet's lease, claimed and issued by this runner, in the same
    /// database: one host's renewal frame names every lease it holds.
    pub(crate) async fn hold_another(&self) -> Other {
        sqlx::query("UPDATE runner.runners SET worker_count = $2 WHERE id = $1")
            .bind(&self.runner)
            .bind(HOST_WORKERS)
            .execute(
                &mut *self
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("a pooled connection"),
            )
            .await
            .expect("the host takes several leases");
        let Seeded { fleet, tenant, .. } =
            seeded_for::<2>(&self.fixtures, ACTOR, REQUEST_JSON).await;
        // The first fleet's placement tag, so this runner may place the second.
        sqlx::query(
            "UPDATE fleet.fleets SET required_tags = \
             (SELECT required_tags FROM fleet.fleets WHERE id = $2) WHERE id = $1",
        )
        .bind(&fleet)
        .bind(&self.fleet)
        .execute(
            &mut *self
                .fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the second fleet takes the first's placement tag");
        self.fixtures.seed_wallet(&tenant, DEEP_POOL).await;
        let acquired = crate::seed::select_fleet(&self.leases, &self.runner, &fleet)
            .await
            .expect("the second fleet is leasable");
        let issued = issue_received(
            &self.leases,
            &self.runner,
            &acquired,
            &tenant,
            self.fixtures.enrolled_at(),
        )
        .await;
        Other {
            lease: issued.lease_id,
            fleet,
            tenant,
            workspace: acquired.workspace_id,
        }
    }
}

/// What the run fee charges for `wall_ms` of runtime, rounded up to the nano.
pub(crate) const fn run_fee(wall_ms: i64) -> i64 {
    (wall_ms * RUN_NANOS_PER_SEC + MS_PER_SEC - 1) / MS_PER_SEC
}

mod plane;
mod slices;
pub(crate) use plane::FIXTURE_KEK_HEX;
