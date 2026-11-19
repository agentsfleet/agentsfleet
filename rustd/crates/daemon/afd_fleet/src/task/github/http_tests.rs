//! One bounded request per call: only `200` and `201` answer, GitHub's rate
//! limits become a cooldown, and a body past two MiB is refused.

use std::time::Duration;

use af_core::clock;
use serde_json::{Value, json};

use super::super::double::{Origin, Reply};
use crate::{Result, error::ErrorKind, task::TaskRefusal};

/// What a remote failure told the caller.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Status(u16),
    Cooldown(u16, jiff::Timestamp),
}

fn answer<T>(result: Result<T>) -> Option<Answer> {
    match result.err()?.kind() {
        ErrorKind::TaskRemoteStatus { status } => Some(Answer::Status(*status)),
        ErrorKind::TaskRemoteCooldown { status, until } => Some(Answer::Cooldown(*status, *until)),
        _ => None,
    }
}

/// The instant a cooldown with `expected` as its status lasts until.
fn cooldown<T>(result: Result<T>, expected: u16) -> Option<jiff::Timestamp> {
    match answer(result)? {
        Answer::Cooldown(status, until) if status == expected => Some(until),
        Answer::Cooldown(..) | Answer::Status(_) => None,
    }
}

fn refusal<T>(result: Result<T>) -> Option<TaskRefusal> {
    match result.err()?.kind() {
        ErrorKind::TaskRefused { reason } => Some(*reason),
        _ => None,
    }
}

const REPOSITORY: &str = "/repos/example/linkwarden";

#[tokio::test]
async fn only_a_read_session_lends_its_credential() {
    let origin = Origin::start(Vec::new()).await;
    let reader = origin.session(true);
    let publisher = origin.session(false);

    assert_eq!(
        reader
            .read_credential()
            .map(|credential| credential.token.as_str().to_owned())
            .ok(),
        Some("native-test-read-token".to_owned())
    );
    assert_eq!(
        refusal(publisher.read_credential()),
        Some(TaskRefusal::InvalidTarget)
    );
}

#[tokio::test]
async fn a_session_past_its_deadline_sends_nothing() {
    let origin = Origin::start(Vec::new()).await;
    let session = origin
        .session(true)
        .with_deadline(jiff::Timestamp::UNIX_EPOCH);

    assert_eq!(
        refusal(session.get::<Value>(REPOSITORY).await),
        Some(TaskRefusal::StaleState)
    );
    assert_eq!(origin.requests(), Vec::<String>::new());
}

#[tokio::test]
async fn a_json_answer_is_read_from_the_fixed_origin_with_its_query() {
    let origin = Origin::start(vec![Reply::json(&json!({"total_count": 0}))]).await;
    let target = "/repos/example/linkwarden/actions/runs?head_sha=abc&per_page=100";

    let read = origin.session(true).get::<Value>(target).await.ok();

    assert_eq!(read, Some(json!({"total_count": 0})));
    assert_eq!(origin.requests(), [format!("GET {target}")]);
}

#[tokio::test]
async fn a_status_other_than_ok_or_created_is_a_remote_status() {
    for (reply, status) in [
        (Reply::status(404), 404),
        (Reply::status(500), 500),
        (
            Reply::status(403).header("x-ratelimit-remaining", "5".to_owned()),
            403,
        ),
    ] {
        let origin = Origin::start(vec![reply]).await;
        let result = origin.session(true).get::<Value>(REPOSITORY).await;
        let code = result.as_ref().err().map(|error| error.code().as_str());

        assert_eq!(code, Some("UZ-DT-010"), "{status}");
        assert_eq!(answer(result), Some(Answer::Status(status)));
        assert_eq!(origin.requests(), [format!("GET {REPOSITORY}")]);
    }
}

/// An exhausted primary limit answers `403` and names the second it resets.
#[tokio::test]
async fn an_exhausted_rate_limit_cools_down_until_its_reset() {
    let reset = clock::now().as_second() + 600;
    let origin = Origin::start(vec![
        Reply::status(403)
            .header("x-ratelimit-remaining", "0".to_owned())
            .header("x-ratelimit-reset", reset.to_string()),
    ])
    .await;

    let result = origin.session(true).get::<Value>(REPOSITORY).await;

    assert_eq!(
        answer(result),
        jiff::Timestamp::from_second(reset)
            .ok()
            .map(|until| Answer::Cooldown(403, until))
    );
}

#[tokio::test]
async fn a_secondary_limit_cools_down_for_its_retry_after() {
    let origin = Origin::start(vec![
        Reply::status(429).header("retry-after", "120".to_owned()),
    ])
    .await;

    let before = clock::now();
    let result = origin.session(true).get::<Value>(REPOSITORY).await;
    let after = clock::now();

    let until = cooldown(result, 429);
    let (earliest, latest) = (
        clock::after(before, Duration::from_secs(120)),
        clock::after(after, Duration::from_secs(120)),
    );
    assert!(
        until.is_some_and(|until| earliest <= until && until <= latest),
        "{until:?}"
    );
}

/// A reset already past says nothing about when to retry, so the cooldown is
/// the five-minute default.
#[tokio::test]
async fn a_limit_without_a_future_instant_cools_down_for_five_minutes() {
    let origin = Origin::start(vec![
        Reply::status(429).header("x-ratelimit-reset", "1".to_owned()),
    ])
    .await;

    let before = clock::now();
    let result = origin.session(true).get::<Value>(REPOSITORY).await;
    let after = clock::now();

    let until = cooldown(result, 429);
    let (earliest, latest) = (
        clock::after(before, Duration::from_secs(300)),
        clock::after(after, Duration::from_secs(300)),
    );
    assert!(
        until.is_some_and(|until| earliest <= until && until <= latest),
        "{until:?}"
    );
}

#[tokio::test]
async fn a_body_past_two_mebibytes_is_refused() {
    let origin = Origin::start(vec![Reply::raw(vec![b' '; 2 * 1024 * 1024 + 1])]).await;

    assert_eq!(
        refusal(origin.session(true).get::<Value>(REPOSITORY).await),
        Some(TaskRefusal::InvalidTarget)
    );
}
