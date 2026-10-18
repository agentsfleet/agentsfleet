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

use crate::queue;
use afd_billing::{Cumulative, Meter, SliceRates};
use afd_core::timing::{LEASE_TTL, MAX_RUNTIME};
use afd_fleet::lease::Renewed;

use crate::report_seed::{Held, held};
use crate::rows::{LAPSE, Row};
use crate::seed::{MODEL, PROVIDER};

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
/// takes `now + LEASE_TTL`, because that is the smaller of the two arms. A
/// lease whose `created_at + MAX_RUNTIME` falls INSIDE that window takes the
/// ceiling instead — and the run gets the remainder of its budget rather than a
/// full fresh TTL. A clamp that took the wrong arm would let a wedged agent
/// renew forever, one TTL at a time, which is the whole failure `MAX_RUNTIME`
/// exists to prevent.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_clamps_to_the_hard_ceiling() {
    let Held {
        fixtures,
        leases,
        runner,
        issued,
        fleet,
        now,
        ..
    } = held().await;
    let lease = issued.lease_id.as_str();

    // An ordinary renewal, well inside the ceiling: the TTL arm wins. Renewal
    // reads the database clock and stamps that instant as the meter's next
    // start, so the deadline is stated against the stamp it wrote.
    let early = leases
        .extend(lease, &runner, free_meter(), now)
        .await
        .expect("the extend must reach the datastore");
    let renewed_at = fixtures.metered_since(&fleet).await;
    assert_eq!(
        early,
        Renewed::Extended {
            expires_at: afd_core::clock::after(renewed_at, LEASE_TTL),
            charged: afd_billing::Nanos::ZERO,
        },
        "with hours of budget left, a renewal takes the full lease TTL"
    );
    assert_eq!(
        fixtures
            .instant(Row::Slot(&fleet), "lease_expires_at")
            .await,
        Some(afd_core::clock::after(renewed_at, LEASE_TTL)),
        "the SLOT moves with the lease — advancing one without the other is what \
         gets a healthy run reclaimed mid-flight"
    );

    // The run has been going almost its whole budget: the ceiling now falls
    // inside the TTL window of the database clock, so the ceiling arm wins.
    let nearly_spent = MAX_RUNTIME.saturating_sub(LEASE_TTL / 2);
    fixtures
        .age(Row::Lease(lease), "created_at", nearly_spent)
        .await;
    let started = fixtures
        .instant(Row::Lease(lease), "created_at")
        .await
        .expect("the lease row carries its start");
    let ceiling = afd_core::clock::after(started, MAX_RUNTIME);

    let clamped = leases
        .extend(lease, &runner, free_meter(), now)
        .await
        .expect("the extend must reach the datastore");
    assert_eq!(
        clamped,
        Renewed::Extended {
            expires_at: ceiling,
            charged: afd_billing::Nanos::ZERO,
        },
        "inside the last TTL of its budget, a run is given the remainder and not a fresh window"
    );
    assert_eq!(
        fixtures
            .instant(Row::Lease(lease), "lease_expires_at")
            .await,
        Some(ceiling),
        "the clamped instant is what the row carries, so the runner's kill deadline is the real one"
    );

    // Past the ceiling by the database clock, the guard's `capped > now`
    // fails and nothing moves.
    fixtures
        .age(Row::Lease(lease), "created_at", LEASE_TTL)
        .await;
    assert_eq!(
        leases
            .extend(lease, &runner, free_meter(), now)
            .await
            .expect("a capped renewal is an answer, not a fault"),
        Renewed::MaxRuntime,
        "past its ceiling a run cannot renew at all — it stops and reports"
    );
    assert_eq!(
        fixtures
            .instant(Row::Lease(lease), "lease_expires_at")
            .await,
        Some(ceiling),
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
    let Held {
        fixtures,
        leases,
        runner: first,
        spare: second,
        issued,
        fleet,
        now,
        ..
    } = held().await;
    let lease = issued.lease_id.as_str();

    // The holder stalls past its TTL and the work is taken back.
    fixtures
        .age(Row::Slot(&fleet), "lease_expires_at", LAPSE)
        .await;
    crate::seed::select_fleet_within_rotations(&leases, &second, &fleet)
        .await
        .expect("a lapsed claim is winnable");

    assert_eq!(
        leases
            .extend(lease, &first, free_meter(), now)
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
    // Funded at issue: the lease must be grantable, or this test would be
    // proving the ISSUE gate and not the renewal one.
    let Held {
        fixtures,
        runner,
        issued,
        fleet,
        tenant,
        now,
        ..
    } = held().await;
    // Priced, so the run has a floor to fail to cover. The catalogue ships
    // EMPTY and `MODEL` is a fixture spelling no admin would curate, so without
    // this the estimate is `Unpriceable`, its floor is zero, a zero balance
    // covers zero, and the CREDITS gate admits — leaving the BUDGET gate to
    // refuse and this test asserting the wrong code for the right outcome.
    fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let lease = issued.lease_id.as_str();
    let original = fixtures
        .instant(Row::Lease(lease), "lease_expires_at")
        .await
        .expect("the issued lease has a deadline");
    let plane = fixtures.plane();

    // The pool runs dry mid-run. Nothing about the lease changes; the gate is
    // read LIVE, which is the property this asserts.
    fixtures.seed_wallet(&tenant, 0).await;

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
        fixtures
            .instant(Row::Lease(lease), "lease_expires_at")
            .await,
        Some(original),
        "a refused renewal advances nothing — the lease expires on its original deadline"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
