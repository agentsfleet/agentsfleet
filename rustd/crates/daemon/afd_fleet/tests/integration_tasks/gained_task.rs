//! A plain chat's run that binds a durable task mid-run keeps renewing.
//!
//! The stream session keeps a plain run's facts from its offer and renews
//! it without a pre-read. Binding a task afterwards sets the run's task, and
//! the renewal statement never renews a task's run, so the frame must hand
//! it to the per-run path, which checks the task's authority.

use af_wire::report::RenewRequest;
use afd_fleet::run::renew::Renewals;
use afd_fleet::task::TaskLifecycle as _;

use super::fixture::{Case, exercise};

/// One kept frame for the case's run, as its stream session sends it.
async fn kept_frame(
    case: &Case,
    plane: &afd_fleet::run::RunService,
) -> afd_fleet::Result<jiff::Timestamp> {
    let held = &case.held;
    let run = &held.issued.run_id;
    let facts = held
        .runs
        .load_for_report(run, &held.runner)
        .await
        .expect("the run reads")
        .expect("the run is the runner's");
    plane
        .renew_batch(Renewals {
            runner_id: &held.runner,
            runs: vec![(run.clone(), RenewRequest::default())],
            funds_due: false,
            now: held.now,
            kept: std::collections::HashMap::from([(run.clone(), facts)]),
        })
        .await
        .into_iter()
        .next()
        .expect("one answer per run")
}

// Value: protects=a chat that creates a durable task keeps its run;
// fails_when=the kept frame treats the run as plain and the statement's
// task guard answers it lost; why_new=renewal takes a plain run's facts from
// its offer; seam=none; no_claim=the task's own authority checks, which the
// per-run path's suites prove.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_run_that_binds_a_task_keeps_renewing() {
    exercise(gained).await;
}

async fn gained(case: &Case) {
    let plane = case.held.fixtures.plane();
    kept_frame(case, &plane)
        .await
        .expect("the plain run renews");

    case.tasks
        .bind(case.run(), &case.plan("gained-task", 1))
        .await
        .expect("the test_run binds a task");
    let renewed = kept_frame(case, &plane).await;

    assert!(renewed.is_ok(), "the task's run renews: {renewed:?}");
}
