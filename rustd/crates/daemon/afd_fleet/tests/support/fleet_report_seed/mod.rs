//! One issued, active lease with a funded tenant behind it.
//!
//! Builds on `fleet_lease_seed` through the claim, narrative log and lease row,
//! so report tests share the same funded starting state.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::sync::LazyLock;

use af_core::id::Uuid7;
use afd_billing::RUN_NANOS_PER_SEC;
use afd_billing::biller::{Biller as _, WorkKey};
use afd_billing::ledger::LedgerBiller;
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

impl Held {
    /// What the billing relay does for this lease: charge every line it left
    /// unsettled. Scoped to the lease, so a test on the shared database never
    /// settles another test's lines.
    pub(crate) async fn relay(&self) {
        let database = &self.fixtures.database;
        let work = WorkKey::FleetLease(self.lease_key.clone());
        let unsettled = afd_billing::ledger::unsettled_work(
            &mut *database.acquire().await.expect("a pooled connection"),
            &work,
        )
        .await
        .expect("the lease's lines read");
        for settlement in unsettled {
            LedgerBiller::on_system_clock(database.clone())
                .settle(&settlement)
                .await
                .expect("the relay charges the lease's lines");
        }
    }

    /// Moves the fleet's live sequence one past the held lease, as a won claim
    /// does before it expires the prior lease, and answers the new sequence.
    pub(crate) async fn supersede(&self) -> u64 {
        let mut connection = self
            .fixtures
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        let live: i64 = sqlx::query_scalar(
            "UPDATE runner.fleet_slots SET fencing_token = fencing_token + 1
             WHERE fleet_id = $1 RETURNING fencing_token",
        )
        .bind(&self.fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the held fleet has an affinity row");
        assert!(
            live > self.fence.as_i64(),
            "the sequence must outrank the held lease, or this test proves nothing"
        );
        u64::try_from(live).expect("a sequence is never negative")
    }

    /// Ages the lease's meter by one [`SLICE`], as a second of runtime would,
    /// and answers the stamp the next settle or renewal measures from.
    ///
    /// Settles and renewals meter up to the database's clock, so a test runs a
    /// slice by moving the meter's start back rather than by naming an instant.
    pub(crate) async fn run_a_slice(&self) -> Timestamp {
        self.fixtures
            .age(
                Row::Lease(&self.issued.lease_id),
                crate::lease_reads::COLUMN_LAST_METERED,
                SLICE,
            )
            .await;
        self.fixtures.metered_since(&self.fleet).await
    }

    /// The wall time between `from` and the meter's stamp now, which is the
    /// slice the last settle or renewal charged for.
    pub(crate) async fn metered_from(&self, from: Timestamp) -> i64 {
        let to = self.fixtures.metered_since(&self.fleet).await;
        af_core::clock::saturating_millis_signed(af_core::clock::elapsed(from, to))
    }
}

/// What the run fee charges for `wall_ms` of runtime, rounded up to the nano.
pub(crate) const fn run_fee(wall_ms: i64) -> i64 {
    (wall_ms * RUN_NANOS_PER_SEC + MS_PER_SEC - 1) / MS_PER_SEC
}

mod plane;
pub(crate) use plane::FIXTURE_KEK_HEX;
