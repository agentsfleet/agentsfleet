//! Malformed runner rows require an isolated database: operator listings are global.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::paging::DEFAULT_LIMIT;
use afd_runner::RunnerEventFilter;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use crate::requests::enrolment;
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn runner_views_report_missing_and_malformed_rows_without_partial_success() {
    let fixtures = Fixtures::create_isolated().await;
    assert_missing_runner_is_not_found(&fixtures).await;

    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            fixtures.enrolled_at(),
        )
        .await
        .expect("the runner enrols");
    overwrite_status(&fixtures, &enrolled.runner_id).await;
    let malformed = fixtures
        .runners()
        .runner_detail(&enrolled.runner_id)
        .await
        .expect_err("an unknown stored state fails the whole detail");
    assert_eq!(malformed.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(malformed.detail(), afd_runner::DETAIL_DATABASE_ERROR);
    insert_unknown_event(&fixtures, &enrolled.runner_id).await;
    let malformed_event = fixtures
        .runners()
        .runner_events(
            &enrolled.runner_id,
            &RunnerEventFilter::default(),
            None,
            DEFAULT_LIMIT,
        )
        .await
        .expect_err("an unknown stored event type fails the whole page");
    assert_eq!(malformed_event.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(malformed_event.detail(), afd_runner::DETAIL_DATABASE_ERROR);
    assert!(std::error::Error::source(&malformed_event).is_some());
    fixtures.cleanup().await;
}

async fn overwrite_status(fixtures: &Fixtures, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE runner.runners SET status = $2 WHERE id = $1::uuid")
        .bind(runner.as_str())
        .bind("unknown_state")
        .execute(&mut *connection)
        .await
        .expect("the malformed fixture state is stored");
}

async fn insert_unknown_event(fixtures: &Fixtures, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO runner.events \
         (id, runner_id, event_type, metadata, last_seen_at, created_at) \
         VALUES ($1::uuid, $2::uuid, $3, $4::jsonb, NULL, now())",
    )
    .bind("0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0c")
    .bind(runner.as_str())
    .bind("unknown_event")
    .bind("{}")
    .execute(&mut *connection)
    .await
    .expect("the malformed fixture event is stored");
}

async fn assert_missing_runner_is_not_found(fixtures: &Fixtures) {
    let missing = Uuid7::parse("0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0b")
        .expect("the missing identifier is canonical");
    let detail_error = fixtures
        .runners()
        .runner_detail(&missing)
        .await
        .expect_err("a missing runner has no detail");
    let event_error = fixtures
        .runners()
        .runner_events(&missing, &RunnerEventFilter::default(), None, DEFAULT_LIMIT)
        .await
        .expect_err("a missing runner has no history");
    for error in [detail_error, event_error] {
        assert_eq!(error.code(), error_code::RUNNER_NOT_FOUND);
        assert_eq!(error.detail(), afd_runner::DETAIL_RUNNER_NOT_FOUND);
    }
}
