//! §3 against live datastores — the renewal's clamp and its coverage gate.
//!
//! Dimension 3.4. Split from `integration_report_settle.rs` because the
//! precondition differs in a way that matters: these tests need a run whose
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

use af_core::id::Uuid7;
use af_core::timing::{MAX_RUNTIME, RUN_TTL};
use afd_fleet::run::sql::meter::Metering;
use afd_fleet::run::usage::Pricing;
use afd_fleet::run::{Renewed, Runs};
use jiff::Timestamp;

use crate::report_seed::{Held, held};
use crate::rows::{LAPSE, Row};
use crate::seed::{MODEL, PROVIDER};

/// One renewal of `run`, reporting no tokens.
pub(crate) async fn extend(
    runs: &Runs,
    run: &Uuid7,
    runner: &Uuid7,
    priced: (&Uuid7, &Pricing),
    now: Timestamp,
) -> afd_fleet::Result<Renewed> {
    let (run_key, pricing) = priced;
    runs.extend(
        Metering {
            run_id: run,
            runner_id: runner,
            requests: &[],
        },
        run_key,
        pricing,
        now,
    )
    .await
}

/// The deadline a renewal granted, when it extended.
pub(crate) const fn deadline(renewed: &Renewed) -> Option<Timestamp> {
    match renewed {
        Renewed::Extended { expires_at, .. } => Some(*expires_at),
        Renewed::MaxRuntime | Renewed::Lost | Renewed::Cancelled => None,
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_clamps_to_the_hard_ceiling() {
    let held = held().await;
    assert_full_ttl(&held).await;
    assert_hard_ceiling(&held).await;
    held.fixtures.cleanup().await;
}

/// Dimension 3.4b — a renewal for a run that is no longer ours is LOST.
///
/// The other terminal verdict, and the one that must not be confused with the
/// cap: `MaxRuntime` says the runner did nothing wrong and its result is still
/// wanted, where `Lost` says the run belongs to somebody else and the result
/// will be refused. A renewal that answered the cap here would have the runner
/// report a result into a run it no longer holds.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_after_reassignment_is_lost() {
    let Held {
        fixtures,
        runs,
        runner: first,
        spare: second,
        issued,
        fleet,
        now,
        run_key,
        pricing,
        ..
    } = held().await;
    let run = &issued.run_id;

    // The holder stalls past its TTL and the work is taken back.
    fixtures
        .age(Row::Thread(&fleet), "run_expires_at", LAPSE)
        .await;
    crate::seed::select_fleet(&runs, &second, &fleet)
        .await
        .expect("a lapsed assignment is winnable");

    assert_eq!(
        extend(&runs, run, &first, (&run_key, &pricing), now)
            .await
            .expect("a lost renewal is an answer, not a fault"),
        Renewed::Lost,
        "the reassignment expired this run, so its former holder must terminate its child"
    );

    fixtures.cleanup().await;
}

/// Dimension 3.4c — an exhausted tenant cannot buy another slice.
///
/// The refusal that makes the clamp above matter: without it a run whose
/// tenant went to zero mid-flight would renew forever on credit nobody has.
///
/// This is the one §3 test that needs the composed [`Plane`] rather than the
/// store, and the reason is the gate's own shape — it reads a WALLET and a
/// fleet ceiling, neither of which the run store can see. The wallet is
/// seeded holding zero rather than left absent, which is a distinction the gate
/// draws deliberately: a tenant with no wallet row at all is ADMITTED, because
/// an unprovisioned tenant is an operator gap and refusing every one of its
/// events would turn that into an outage.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renew_coverage_refuses_an_empty_wallet() {
    // Funded at issue: the run must be grantable, or this test would be
    // proving the ISSUE gate and not the renewal one.
    let Held {
        fixtures,
        runner,
        issued,
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
    let run = &issued.run_id;
    let original = issued_deadline(&fixtures, run).await;
    let plane = fixtures.plane();

    // The pool runs dry mid-run. Nothing about the run changes; the gate is
    // read LIVE, which is the property this asserts.
    fixtures.seed_wallet(&tenant, 0).await;

    let refusal = plane
        .renew(&runner, run, af_wire::report::RenewRequest::default(), now)
        .await
        .expect_err("an exhausted tenant cannot fund another slice");
    assert_eq!(
        refusal.code(),
        af_core::error_code::RUN_RENEWAL_NO_CREDITS,
        "the runner is told WHICH pool ran dry: the tenant's balance, not the fleet's ceiling"
    );

    assert_eq!(
        fixtures.instant(Row::Run(run), "run_expires_at").await,
        Some(original),
        "a refused renewal advances nothing — the run expires on its original deadline"
    );

    fixtures.cleanup().await;
}

async fn issued_deadline(fixtures: &crate::support::Fixtures, run: &Uuid7) -> Timestamp {
    fixtures
        .instant(Row::Run(run), "run_expires_at")
        .await
        .expect("the issued run has a deadline")
}

async fn assert_full_ttl(held: &Held) {
    let fixtures = &held.fixtures;
    let run = &held.issued.run_id;
    let early = extend(
        &held.runs,
        run,
        &held.runner,
        (&held.run_key, &held.pricing),
        held.now,
    )
    .await
    .expect("the extend must reach the datastore");
    let renewed_at = fixtures.metered_since(&held.fleet).await;
    assert_eq!(
        deadline(&early),
        Some(af_core::clock::after(renewed_at, RUN_TTL)),
        "with hours of budget left, a renewal takes the full run TTL"
    );
    assert_eq!(
        fixtures
            .instant(Row::Thread(&held.fleet), "run_expires_at")
            .await,
        Some(af_core::clock::after(renewed_at, RUN_TTL)),
        "the THREAD moves with the run — advancing one without the other is what \
         gets a healthy run reassigned mid-flight"
    );
}

async fn assert_hard_ceiling(held: &Held) {
    let fixtures = &held.fixtures;
    let run = &held.issued.run_id;
    // The run has been going almost its whole budget: the ceiling now falls
    // inside the TTL window of the database clock, so the ceiling arm wins.
    let nearly_spent = MAX_RUNTIME.saturating_sub(RUN_TTL / 2);
    fixtures
        .age(Row::Run(run), "created_at", nearly_spent)
        .await;
    let started = fixtures
        .instant(Row::Run(run), "created_at")
        .await
        .expect("the run row carries its start");
    let ceiling = af_core::clock::after(started, MAX_RUNTIME);

    let clamped = extend(
        &held.runs,
        run,
        &held.runner,
        (&held.run_key, &held.pricing),
        held.now,
    )
    .await
    .expect("the extend must reach the datastore");
    assert_eq!(
        deadline(&clamped),
        Some(ceiling),
        "inside the last TTL of its budget, a run is given the remainder and not a fresh window"
    );
    assert_eq!(
        fixtures.instant(Row::Run(run), "run_expires_at").await,
        Some(ceiling),
        "the clamped instant is what the row carries, so the runner's kill deadline is the real one"
    );

    assert_past_ceiling(held, ceiling).await;
}

async fn assert_past_ceiling(held: &Held, ceiling: Timestamp) {
    let fixtures = &held.fixtures;
    let run = &held.issued.run_id;
    // Past the ceiling by the database clock, the guard's `capped > now`
    // fails and nothing moves.
    fixtures.age(Row::Run(run), "created_at", RUN_TTL).await;
    assert_eq!(
        extend(
            &held.runs,
            run,
            &held.runner,
            (&held.run_key, &held.pricing),
            held.now
        )
        .await
        .expect("a capped renewal is an answer, not a fault"),
        Renewed::MaxRuntime,
        "past its ceiling a run cannot renew at all — it stops and reports"
    );
    assert_eq!(
        fixtures.instant(Row::Run(run), "run_expires_at").await,
        Some(ceiling),
        "and the refused renewal advanced nothing"
    );
}

// Value: protects=a run at its runtime ceiling with request lines to settle
// is told it reached the ceiling; fails_when=the settled lines turn the
// verdict into a stop the runner reads as a lost run; why_new=the refusal
// arm settled known lines and replaced every verdict but a cancel; seam=none;
// no_claim=what the settled lines charge.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_run_at_its_ceiling_with_lines_to_settle_reaches_the_ceiling() {
    let held = held().await;
    let run = &held.issued.run_id;
    held.fixtures
        .age(Row::Run(run), "created_at", MAX_RUNTIME + RUN_TTL)
        .await;
    let requests = [af_wire::usage::RequestUsage {
        seq: 0,
        at: held.now,
        input_tokens: 10,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 10,
    }];

    let renewed = held
        .runs
        .extend(
            Metering {
                run_id: run,
                runner_id: &held.runner,
                requests: &requests,
            },
            &held.run_key,
            &held.pricing,
            held.now,
        )
        .await
        .expect("the extend must reach the datastore");

    assert!(
        matches!(renewed, Renewed::MaxRuntime),
        "the ceiling, not a lost run: {renewed:?}"
    );
    held.fixtures.cleanup().await;
}
