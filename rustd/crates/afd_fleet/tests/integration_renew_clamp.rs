//! §3 against live datastores — the renewal's clamp and its coverage gate.
//!
//! Dimension 3.4. Split from `integration_report_settle.rs` because the
//! precondition differs in a way that matters: these tests need a lease whose
//! `created_at` is far enough back to reach the hard ceiling, and a wallet that
//! is deliberately empty. A suite that seeded both for every test would make
//! the report's row-parity assertions depend on state they do not care about.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_report_reads.rs"]
mod report_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_report_seed.rs"]
mod report_seed;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL_MS, MAX_RUNTIME_MS};
use afd_fleet::lease::{Billed, Delivery, Renewed};
use afd_fleet::money::{Cumulative, Meter, SliceRates};

use self::requests::ENROLLED_AT;
use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;

/// A pool deep enough that nothing under test clamps against it.
const DEEP_POOL: i64 = 1_000_000_000_000;

/// Rates that charge nothing at all.
///
/// These tests are about the DEADLINE arithmetic, not the money: a slice that
/// charged would make every assertion below depend on the wallet's depth as
/// well as on the clamp, and the charge has its own coverage in the report
/// suite next door.
const FREE: SliceRates = SliceRates {
    run_nanos_per_sec: 0,
    input_nanos_per_mtok: 0,
    cached_input_nanos_per_mtok: 0,
    output_nanos_per_mtok: 0,
};

/// A meter that prices every slice at zero.
fn free_meter() -> Meter {
    Meter {
        cumulative: Cumulative::default(),
        rates: FREE,
    }
}

/// Dimension 3.4a — a renewal near the ceiling is clamped to it, not past it.
///
/// Three renewals, and the middle one is the assertion. An ordinary renewal
/// takes `now + LEASE_TTL_MS`, because that is the smaller of the two arms. A
/// lease whose `created_at + MAX_RUNTIME_MS` falls INSIDE that window takes the
/// ceiling instead — and the run gets the remainder of its budget rather than a
/// full fresh TTL. A clamp that took the wrong arm would let a wedged agent
/// renew forever, one TTL at a time, which is the whole failure `MAX_RUNTIME_MS`
/// exists to prevent.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_clamps_to_the_hard_ceiling() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;

    let acquired = leases
        .select(&runner, now)
        .await
        .expect("the assignment pass must not fault")
        .expect("the seeded fleet is leasable");
    leases
        .record_received(&acquired, now)
        .await
        .expect("the narrative log must open");
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let issued = leases
        .issue(
            &runner,
            &acquired,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written");
    let lease = issued.lease_id.as_str();

    // An ordinary renewal, well inside the ceiling: the TTL arm wins.
    let early = leases
        .extend(lease, &runner, free_meter(), now)
        .await
        .expect("the extend must reach the datastore");
    assert_eq!(
        early,
        Renewed::Extended {
            expires_at: now.saturating_add_millis(LEASE_TTL_MS),
            charged: afd_fleet::money::Nanos::ZERO,
        },
        "with hours of budget left, a renewal takes the full lease TTL"
    );
    assert_eq!(
        fixtures.affinity_column(&fleet, "leased_until").await,
        Some(
            now.saturating_add_millis(LEASE_TTL_MS)
                .as_millis()
                .to_string()
        ),
        "the SLOT moves with the lease — advancing one without the other is what \
         gets a healthy run reclaimed mid-flight"
    );

    // The run has been going almost its whole budget: the ceiling now falls
    // inside the TTL window, so the ceiling arm wins.
    let nearly_spent = ENROLLED_AT - MAX_RUNTIME_MS + (LEASE_TTL_MS / 2);
    fixtures.backdate_lease(lease, nearly_spent).await;
    let ceiling = UnixMillis::from_millis(nearly_spent + MAX_RUNTIME_MS);

    let clamped = leases
        .extend(lease, &runner, free_meter(), now)
        .await
        .expect("the extend must reach the datastore");
    assert_eq!(
        clamped,
        Renewed::Extended {
            expires_at: ceiling,
            charged: afd_fleet::money::Nanos::ZERO,
        },
        "inside the last TTL of its budget, a run is given the remainder and not a fresh window"
    );
    assert_eq!(
        fixtures.lease_column(lease, "lease_expires_at").await,
        Some(ceiling.as_millis().to_string()),
        "the clamped instant is what the row carries, so the runner's kill deadline is the real one"
    );

    // Past the ceiling, the guard's `capped > now` fails and nothing moves.
    let expired = ceiling.saturating_add_millis(1);
    assert_eq!(
        leases
            .extend(lease, &runner, free_meter(), expired)
            .await
            .expect("a capped renewal is an answer, not a fault"),
        Renewed::MaxRuntime,
        "past its ceiling a run cannot renew at all — it stops and reports"
    );
    assert_eq!(
        fixtures.lease_column(lease, "lease_expires_at").await,
        Some(ceiling.as_millis().to_string()),
        "and the refused renewal advanced nothing"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Dimension 3.4b — a renewal for a lease that is no longer ours is LOST.
///
/// The other terminal verdict, and the one that must not be confused with the
/// cap: `MaxRuntime` says the runner did nothing wrong and its result is still
/// wanted, where `Lost` says the lease belongs to somebody else and the result
/// will be refused. A renewal that answered the cap here would have the runner
/// report a result into a lease it no longer holds.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_after_reclaim_is_lost() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;

    let acquired = leases
        .select(&first, now)
        .await
        .expect("the assignment pass must not fault")
        .expect("the seeded fleet is leasable");
    assert_eq!(
        leases
            .record_received(&acquired, now)
            .await
            .expect("the narrative log must open"),
        Delivery::First,
        "a newly leased event has no row yet"
    );
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let issued = leases
        .issue(
            &first,
            &acquired,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written");
    let lease = issued.lease_id.as_str();

    // The holder stalls past its TTL and the work is taken back.
    let lapsed = now.saturating_add_millis(LEASE_TTL_MS + 1);
    leases
        .select(&second, lapsed)
        .await
        .expect("the reclaim pass must not fault")
        .expect("a lapsed claim is winnable");

    assert_eq!(
        leases
            .extend(lease, &first, free_meter(), lapsed)
            .await
            .expect("a lost renewal is an answer, not a fault"),
        Renewed::Lost,
        "the reclaim expired this lease, so its former holder must terminate its child"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Dimension 3.4c — an exhausted tenant cannot buy another slice.
///
/// The refusal that makes the clamp above matter: without it a run whose
/// tenant went to zero mid-flight would renew forever on credit nobody has.
///
/// This is the one §3 test that needs the composed [`Plane`] rather than the
/// store, and the reason is the gate's own shape — it reads a WALLET and a
/// fleet ceiling, neither of which the lease store can see. The wallet is
/// seeded holding zero rather than left absent, which is a distinction the gate
/// draws deliberately: a tenant with no wallet row at all is ADMITTED, because
/// an unprovisioned tenant is an operator gap and refusing every one of its
/// events would turn that into an outage.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_coverage_refuses_an_empty_wallet() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    // Funded at issue: the lease must be grantable, or this test would be
    // proving the ISSUE gate and not the renewal one.
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;

    let acquired = leases
        .select(&runner, now)
        .await
        .expect("the assignment pass must not fault")
        .expect("the seeded fleet is leasable");
    leases
        .record_received(&acquired, now)
        .await
        .expect("the narrative log must open");
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let issued = leases
        .issue(
            &runner,
            &acquired,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written");
    let lease = issued.lease_id.as_str();
    let plane = fixtures.plane();

    // The pool runs dry mid-run. Nothing about the lease changes; the gate is
    // read LIVE, which is the property this asserts.
    fixtures.seed_wallet(&tenant, 0, ENROLLED_AT).await;

    let refusal = plane
        .renew(
            &runner,
            lease,
            afd_wire::report::RenewRequest::default(),
            now,
        )
        .await
        .expect_err("an exhausted tenant cannot fund another slice");
    assert_eq!(
        refusal.code(),
        afd_core::error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
        "the runner is told WHICH pool ran dry: the tenant's balance, not the fleet's ceiling"
    );

    assert_eq!(
        fixtures.lease_column(lease, "lease_expires_at").await,
        Some(acquired.leased_until.as_millis().to_string()),
        "a refused renewal advances nothing — the lease expires on its original deadline"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
