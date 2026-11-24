//! What a reservation's reads and its one issue statement promise: a saved call answers
//! `existing` before any money check, the reservation needs no row to itself, a renewal
//! gets through a storm of them, and it costs at most eighteen statements with every lock
//! taken by the one that inserts the call.

use std::time::Instant;

use af_core::error_code;
use af_wire::durable_task::{DecisionAssessment, DecisionCallState};
use afd_db::{config::DbRole, test_util::statements};
use afd_fleet::{
    durable_task::{DurableTaskLifecycle as _, DurableTasks},
    run::Runs,
};
use serde_json::json;
use sqlx::Connection as _;

use super::{
    super::fixture::{Case, exercise},
    concurrency::{Operations, contend},
    fixture::{Decisions, known, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replay_of_a_saved_call_answers_existing_when_funding_would_refuse() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        unfunded(&fixture).await;
        overcharged(&fixture).await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

/// With the wallet empty, the saved call's own operation and another operation of its
/// assessment both answer `existing`: the lookups run before the wallet is read.
async fn unfunded(fixture: &Decisions<'_>) {
    let case = fixture.case;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    let expected = json!({"status":"existing", "call_id":body.get("call_id"), "state":"issued"});
    let balance = case
        .held
        .fixtures
        .balance(&case.held.tenant)
        .await
        .expect("a funded payer");
    case.held.fixtures.seed_wallet(&case.held.tenant, 0).await;
    let another = fixture.reservation(DecisionAssessment::Message, None);
    for replay in [&input, &another] {
        assert_eq!(fixture.reserve(replay).await, (false, expected.clone()));
    }
    assert_eq!(fixture.counts().await, (1, 1, 0));
    case.held
        .fixtures
        .seed_wallet(&case.held.tenant, balance)
        .await;
}

/// A call charged above its reservation blocks a new assessment of the run, and still
/// replays for its own operation and for another operation of its assessment.
async fn overcharged(fixture: &Decisions<'_>) {
    let task = fixture.bind_goal().await;
    fixture.reprice().await;
    let input = fixture.goal_reservation(&task);
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    let call = body
        .get("call_id")
        .and_then(serde_json::Value::as_str)
        .expect("the goal's call");
    let recorded = fixture.record(call, known(&request(&body))).await;
    assert_eq!(recorded.state, DecisionCallState::Known);
    assert_eq!(recorded.overage_nanos.as_deref(), Some("20000002"));

    let mut another = (*input).clone();
    another.operation_id = afd_db::test_util::mint_id().into();
    let another = garde::Unvalidated::new(another)
        .validate()
        .expect("another operation of the same assessment");
    for replay in [&input, &another] {
        let (issued, saved) = fixture.reserve(replay).await;
        assert!(!issued);
        assert_eq!(saved.get("state"), Some(&json!("known")));
        assert_eq!(saved.get("call_id"), Some(&json!(call)));
    }
    let refused = fixture
        .tasks
        .reserve_decision(fixture.case.run(), &fixture.goal_reservation(&task))
        .await
        .expect_err("an overcharged run takes no new assessment");
    assert_eq!(refused.code(), error_code::DURABLE_TASK_LIMIT_EXHAUSTED);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reservation_issues_while_a_reader_holds_the_thread() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let mut holder = case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection");
        let mut reading = holder.begin().await.expect("the reader's transaction");
        sqlx::query("SELECT fleet_id FROM thread.threads WHERE fleet_id = $1 FOR SHARE")
            .bind(&case.held.fleet)
            .execute(&mut *reading)
            .await
            .expect("a receipt's read holds the thread shared");

        let input = fixture.reservation(DecisionAssessment::Message, None);
        let (issued, _body) = fixture.reserve(&input).await;

        assert!(issued, "a shared thread does not hold the reservation up");
        assert_eq!(fixture.counts().await, (1, 1, 0));
        reading.rollback().await.expect("the reader lets go");
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_completes_during_a_reservation_storm() {
    exercise(async |case: &Case| {
        let mut fixture = Decisions::create(case).await;
        // The storm on a pool of its own: the renewal then waits for row locks only,
        // never for a pooled connection behind a hundred callers.
        let pool = case.held.fixtures.open_with(DbRole::Api, &[]).await;
        fixture.tasks = DurableTasks::new(
            Runs::new(
                pool.clone(),
                case.held.fixtures.bus().clone(),
                af_crypto::entropy::Entropy::new(),
            ),
            case.turns.clone(),
            fixture.dependencies.clone(),
        );
        let input = fixture.reservation(DecisionAssessment::Message, None);
        let runs = [&case.held.issued.run_id];
        let mut waits = Vec::new();
        let replies = {
            let mut storm = Box::pin(contend(&fixture, &input, Operations::Same));
            loop {
                let began = Instant::now();
                tokio::select! {
                    replies = &mut storm => break replies,
                    answers = crate::integration_renew_batch::frame(&case.held, &runs, false) => {
                        let granted = answers.into_iter().next().expect("one answer for one run");
                        granted.expect("the renewal is granted under the login's lock bound");
                        waits.push(began.elapsed());
                    }
                }
            }
        };
        assert!(!waits.is_empty(), "a renewal completed while the storm ran");
        assert_eq!(replies.iter().filter(|(issued, _)| *issued).count(), 1);
        assert_eq!(fixture.counts().await, (1, 1, 0));
        println!(
            "renewals during the storm: {}, longest {:?}",
            waits.len(),
            waits.iter().max()
        );
        pool.close().await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decision_reservation_runs_at_most_eighteen_statements() {
    exercise(async |case: &Case| {
        let owner = case.held.fixtures.open_with(DbRole::Migrator, &[]).await;
        statements::install(&owner).await;
        let fixture = Decisions::create(case).await;
        let input = fixture.reservation(DecisionAssessment::Message, None);

        let begun = statements::transactions_begun(&owner).await;
        let window = statements::Window::open(&owner).await;
        let (issued, _body) = fixture.reserve(&input).await;
        let ran = window.close().await;
        let begun = statements::transactions_begun(&owner).await - begun;

        assert!(issued);
        println!(
            "a reservation ran {} statements in {begun} transactions",
            ran.total()
        );
        // The replay peek and the vault read each open one. A third would put the issue
        // statement in a transaction, and its locks would then outlive it.
        assert_eq!(begun, 2, "the issue statement runs in no transaction");
        assert!(
            ran.total() <= 18,
            "{} statements: {:#?}",
            ran.total(),
            ran.statements()
        );
        assert_eq!(ran.calls_containing("INSERT INTO runner.decision_calls"), 1);
        for (text, _calls) in ran.statements() {
            let inserts_the_call = text.contains("INSERT INTO runner.decision_calls");
            assert!(
                inserts_the_call || !(text.contains("FOR SHARE") || text.contains("FOR KEY SHARE")),
                "only the issue statement locks: {text}"
            );
            assert!(
                !(text.contains("FOR UPDATE") || text.contains("FOR NO KEY UPDATE")),
                "no statement takes a writer's lock: {text}"
            );
        }
        owner.close().await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
