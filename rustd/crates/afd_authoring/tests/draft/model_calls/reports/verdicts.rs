//! A runner's verdict is held to the daemon's evidence: timed out only past
//! the attempt's deadline, completed only when no dispatch failed.

use super::{Attempt, Fixture, counts, fixture, report};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{FailureReason, Outcome, Report};
use std::sync::Arc;

/// Moves the attempt's wall-clock deadline behind database time, leaving its
/// lease live.
const PAST_DEADLINE: &str = "UPDATE authoring.attempts \
     SET expires_at=date_trunc('milliseconds',clock_timestamp())-interval '1 second' \
     WHERE id=$1::uuid";

/// The attempt's stored outcome.
const OUTCOME: &str = "SELECT outcome FROM authoring.attempts WHERE id=$1::uuid";

fn timed_out() -> Report {
    let mut request = report();
    let Report::Terminal { accounting, .. } = &mut request else {
        unreachable!()
    };
    accounting.outcome = Outcome::TimedOut;
    accounting.failure_reason = Some(FailureReason::AttemptTimedOut);
    request
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_timed_out_verdict_settles_only_past_the_attempts_deadline() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(fixture::Provider::known())).await;
    let early = f
        .drafts
        .report(&f.runner, &a.lease, timed_out())
        .await
        .expect_err("the deadline is still ahead");
    assert_eq!(early.code(), error_code::INVALID_REQUEST);
    assert_eq!(counts(&f, &a).await, (0, 0, 0), "nothing settled early");
    sqlx::query(PAST_DEADLINE)
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("the attempt ran out of wall clock");
    let ack = f
        .drafts
        .report(&f.runner, &a.lease, timed_out())
        .await
        .expect("a timeout past the deadline settles");
    assert!(ack.settled);
    assert_eq!(ack.state, Outcome::TimedOut);
    let outcome: Option<String> = sqlx::query_scalar(OUTCOME)
        .bind(a.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("the settled attempt");
    assert_eq!(outcome.as_deref(), Some("timed_out"));
    assert_eq!(
        counts(&f, &a).await,
        (1, 1, 0),
        "settled, with no private answer"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_completed_verdict_cannot_follow_a_failed_dispatch() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = fixture::Provider::known();
    stub.status = 503;
    let calls = fixture::calls(&f, &a, Arc::new(stub)).await;
    calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect_err("the provider failed with known usage");
    let refused = f
        .drafts
        .report(&f.runner, &a.lease, report())
        .await
        .expect_err("a failed dispatch is no completed answer");
    assert_eq!(refused.code(), error_code::INVALID_REQUEST);
    assert_eq!(counts(&f, &a).await, (0, 0, 0));
    f.cleanup().await;
}
