//! A plain chat's lease that binds a durable task mid-run keeps renewing.
//!
//! The stream session keeps a plain lease's facts from its offer and renews
//! it without a pre-read. Binding a task afterwards sets the lease's task, and
//! the renewal statement never renews a task's lease, so the frame must hand
//! it to the per-lease path, which checks the task's authority.

use af_wire::report::RenewRequest;
use afd_fleet::lease::renew::Renewals;
use afd_fleet::task::TaskLifecycle as _;

use super::fixture::{Case, exercise};

/// One kept frame for the case's lease, as its stream session sends it.
async fn kept_frame(
    case: &Case,
    plane: &afd_fleet::lease::LeaseService,
) -> afd_fleet::Result<(jiff::Timestamp, afd_billing::Nanos)> {
    let held = &case.held;
    let lease = &held.issued.lease_id;
    let facts = held
        .leases
        .load_for_report(lease, &held.runner)
        .await
        .expect("the lease reads")
        .expect("the lease is the runner's");
    plane
        .renew_batch(Renewals {
            runner_id: &held.runner,
            leases: vec![(lease.clone(), RenewRequest::default())],
            funds_due: false,
            now: held.now,
            kept: std::collections::HashMap::from([(lease.clone(), facts)]),
        })
        .await
        .into_iter()
        .next()
        .expect("one answer per lease")
}

// Value: protects=a chat that creates a durable task keeps its lease;
// fails_when=the kept frame treats the lease as plain and the statement's
// task guard answers it lost; why_new=renewal takes a plain lease's facts from
// its offer; seam=none; no_claim=the task's own authority checks, which the
// per-lease path's suites prove.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_lease_that_binds_a_task_keeps_renewing() {
    exercise(gained).await;
}

async fn gained(case: &Case) {
    let plane = case.held.fixtures.plane();
    kept_frame(case, &plane)
        .await
        .expect("the plain lease renews");

    case.tasks
        .bind(case.lease(), &case.plan("gained-task", 1))
        .await
        .expect("the run binds a task");
    let renewed = kept_frame(case, &plane).await;

    assert!(renewed.is_ok(), "the task's lease renews: {renewed:?}");
}
