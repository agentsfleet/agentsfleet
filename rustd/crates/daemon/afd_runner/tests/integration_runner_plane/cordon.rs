//! Automatic quarantine is independent of operator intent and lease expiry.

use af_core::{clock, id::Uuid7, timing::RUNNER_OFFLINE_AFTER};
use af_crypto::entropy::Entropy;
use afd_api_wire::{admin::RunnerEventType, operator::CordonReason};
use afd_runner::{
    capacity::authority::{Authority, Decision},
    sweep::{Sweep as _, liveness::Liveness},
};

use super::{Fixture, enrolment, refusal::Refusal};

async fn connected(fixture: &mut Fixture) -> Authority {
    let enrolled = fixture
        .runners
        .register(&enrolment(), "checkpoint-cordon-test", clock::now())
        .await
        .expect("enrolment");
    fixture.enrolled.push(enrolled.runner_id.clone());
    let owner = match fixture
        .runners
        .acquire_connection(
            &enrolled.runner_id,
            enrolled.token.digest().clone(),
            None,
            clock::now(),
        )
        .await
        .expect("opening")
    {
        Decision::Allowed(owner) => Some(owner),
        Decision::Refused(_) => None,
    };
    owner.expect("a fresh host acquires its connection")
}

async fn silence(fixture: &Fixture, runner: &Uuid7) {
    sqlx::query(
        "UPDATE runner.runners SET last_seen_at = clock_timestamp() - $2::interval WHERE id = $1",
    )
    .bind(runner)
    .bind(RUNNER_OFFLINE_AFTER.saturating_mul(2))
    .execute(&mut *fixture.database.acquire().await.expect("connection"))
    .await
    .expect("age heartbeat");
}

async fn quarantined(fixture: &Fixture, runner: &Uuid7) {
    silence(fixture, runner).await;
    let sweep = Liveness::new(fixture.database.clone(), Entropy::new());
    for _ in 0..64 {
        sweep.sweep().await.expect("liveness sweep");
        if fixture
            .runners
            .platform()
            .runner_detail(runner)
            .await
            .expect("detail")
            .item()
            .cordon()
            .is_some()
        {
            return;
        }
    }
    assert!(
        fixture
            .runners
            .platform()
            .runner_detail(runner)
            .await
            .expect("detail")
            .item()
            .cordon()
            .is_some(),
        "bounded passes reach this silent host"
    );
}

async fn beat(fixture: &Fixture, owner: &Authority) {
    assert!(matches!(
        fixture
            .runners
            .heartbeat_connected(owner, &afd_runner::NO_REPORT, None, clock::now())
            .await
            .expect("heartbeat"),
        Decision::Allowed(_)
    ));
}

async fn prove_elapsed(fixture: &Fixture, runner: &Uuid7) {
    // Advance only the proof's start; last_seen remains a current proved beat.
    sqlx::query("UPDATE runner.runners SET cordon_healthy_since_at = clock_timestamp() - $2::interval WHERE id = $1")
        .bind(runner).bind(af_core::timing::UNCORDON_AFTER + std::time::Duration::from_secs(1))
        .execute(&mut *fixture.database.acquire().await.expect("connection")).await.expect("age proof");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_silent_host_is_cordoned_with_its_offline_record() {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    quarantined(&fixture, owner.runner()).await;
    let detail = fixture
        .runners
        .platform()
        .runner_detail(owner.runner())
        .await
        .expect("operator detail");
    let cordon = detail.item().cordon().expect("automatic cordon");
    assert_eq!(cordon.reason, CordonReason::MissedHeartbeats);
    assert_eq!(
        detail.item().status(),
        afd_api_wire::admin::AdminState::Active
    );
    Liveness::new(fixture.database.clone(), Entropy::new())
        .sweep()
        .await
        .expect("repeat");
    let history = fixture.history(owner.runner()).await;
    assert_eq!(
        history
            .iter()
            .filter(|event| event.event_type == RunnerEventType::RunnerCordoned)
            .count(),
        1
    );
    assert_eq!(
        history
            .iter()
            .filter(|event| event.event_type == RunnerEventType::RunnerOffline)
            .count(),
        1
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_refused_cordon_leaves_no_offline_record() {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    silence(&fixture, owner.runner()).await;
    let runner = owner.runner();
    let refusal = Refusal::install(
        &fixture.lane.url(),
        "runner.runners",
        "cordon",
        runner.as_str(),
        &format!("NEW.id = '{runner}'::uuid AND NEW.cordon_reason IS NOT NULL"),
        "RAISE EXCEPTION 'fixture refusal'",
    )
    .await
    .expect("refusal trigger");
    Liveness::new(fixture.database.clone(), Entropy::new())
        .sweep()
        .await
        .expect_err("the rejected cordon must roll back its transaction");
    assert!(fixture.history(runner).await.iter().all(|event| !matches!(
        event.event_type,
        RunnerEventType::RunnerOffline | RunnerEventType::RunnerCordoned
    )));
    drop(refusal);
    quarantined(&fixture, runner).await;
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_automatic_lift_requires_continuous_valid_beats() {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    quarantined(&fixture, owner.runner()).await;
    beat(&fixture, &owner).await;
    assert!(
        fixture
            .runners
            .platform()
            .runner_detail(owner.runner())
            .await
            .expect("detail")
            .item()
            .cordon()
            .is_some()
    );
    prove_elapsed(&fixture, owner.runner()).await;
    beat(&fixture, &owner).await;
    assert!(
        fixture
            .runners
            .platform()
            .runner_detail(owner.runner())
            .await
            .expect("detail")
            .item()
            .cordon()
            .is_none()
    );
    beat(&fixture, &owner).await;
    assert_eq!(
        fixture
            .history(owner.runner())
            .await
            .iter()
            .filter(|event| event.event_type == RunnerEventType::RunnerUncordoned)
            .count(),
        1
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn missed_cadence_resets_an_old_healthy_proof() {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    quarantined(&fixture, owner.runner()).await;
    beat(&fixture, &owner).await;
    prove_elapsed(&fixture, owner.runner()).await;
    sqlx::query(
        "UPDATE runner.runners SET last_seen_at = clock_timestamp() - $2::interval WHERE id = $1",
    )
    .bind(owner.runner())
    .bind(af_core::timing::HEARTBEAT_INTERVAL.saturating_mul(2))
    .execute(&mut *fixture.database.acquire().await.expect("connection"))
    .await
    .expect("missed beat");
    beat(&fixture, &owner).await;
    assert!(
        fixture
            .runners
            .platform()
            .runner_detail(owner.runner())
            .await
            .expect("detail")
            .item()
            .cordon()
            .is_some()
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn unproved_liveness_resets_health_and_never_lifts() {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    quarantined(&fixture, owner.runner()).await;
    beat(&fixture, &owner).await;
    prove_elapsed(&fixture, owner.runner()).await;
    fixture
        .runners
        .heartbeat(owner.runner(), &afd_runner::NO_REPORT, None, clock::now())
        .await
        .expect("legacy liveness");
    assert_proof_reset(&fixture, owner.runner()).await;
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_failed_lift_can_record_liveness_without_proving_healthy_return() {
    let mut fixture = Fixture::isolated().await;
    let owner = connected(&mut fixture).await;
    let runner = owner.runner();
    quarantined(&fixture, runner).await;
    beat(&fixture, &owner).await;
    prove_elapsed(&fixture, runner).await;
    let refusal = Refusal::install(
        &fixture.lane.url(),
        "runner.runners",
        "lift",
        runner.as_str(),
        &format!("NEW.id = '{runner}'::uuid AND OLD.cordon_reason IS NOT NULL AND NEW.cordon_reason IS NULL"),
        "RAISE EXCEPTION 'fixture refuses lift'",
    )
    .await
    .expect("refusal trigger");
    beat(&fixture, &owner).await;
    assert_proof_reset(&fixture, runner).await;
    assert!(
        fixture
            .history(runner)
            .await
            .iter()
            .all(|event| event.event_type != RunnerEventType::RunnerUncordoned)
    );
    drop(refusal);
    beat(&fixture, &owner).await;
    prove_elapsed(&fixture, runner).await;
    beat(&fixture, &owner).await;
    assert_eq!(
        fixture
            .history(runner)
            .await
            .iter()
            .filter(|event| event.event_type == RunnerEventType::RunnerUncordoned)
            .count(),
        1
    );
    fixture.cleanup().await;
}

async fn assert_proof_reset(fixture: &Fixture, runner: &Uuid7) {
    let started: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar("SELECT cordon_healthy_since_at FROM runner.runners WHERE id = $1")
            .bind(runner)
            .fetch_one(&mut *fixture.database.acquire().await.expect("connection"))
            .await
            .expect("proof");
    assert!(started.is_none());
    assert!(
        fixture
            .runners
            .platform()
            .runner_detail(runner)
            .await
            .expect("detail")
            .item()
            .cordon()
            .is_some()
    );
}
