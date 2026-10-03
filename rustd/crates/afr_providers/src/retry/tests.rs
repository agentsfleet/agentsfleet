#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

use super::{Unanswered, WAIT_CEILING, retry_after};

const BACKOFF: Option<Duration> = Some(Duration::from_secs(1));

/// An answer with `code` and an empty body, asking for `retry_after`.
fn status(code: StatusCode, retry_after: Option<Duration>) -> Unanswered {
    let answer = http::Response::builder().status(code).body("").unwrap();
    Unanswered::Status {
        response: Box::new(reqwest::Response::from(answer)),
        retry_after,
    }
}

fn header(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_str(value).unwrap());
    headers
}

#[test]
fn should_retry_a_rate_limit_and_a_server_fault_and_nothing_else_answered() {
    assert!(status(StatusCode::TOO_MANY_REQUESTS, None).retryable());
    assert!(status(StatusCode::SERVICE_UNAVAILABLE, None).retryable());
    assert!(!status(StatusCode::UNAUTHORIZED, None).retryable());
    assert!(!status(StatusCode::BAD_REQUEST, None).retryable());
}

#[tokio::test]
async fn should_retry_a_send_that_could_not_connect() {
    // Port 1 is reserved and nothing listens on it, so the connect is refused.
    let refused = reqwest::Client::new()
        .get("http://127.0.0.1:1/")
        .send()
        .await
        .unwrap_err();

    let unanswered = Unanswered::Transport(refused);

    assert!(unanswered.retryable());
    assert_eq!(unanswered.status(), None);
    assert!(
        unanswered.into_answer().unwrap_err().is_connect(),
        "the transport's own failure, for rig to read"
    );
}

#[test]
fn should_wait_what_the_provider_asks_and_else_the_backoff() {
    let asked = status(StatusCode::TOO_MANY_REQUESTS, Some(Duration::from_secs(3)));
    let unasked = status(StatusCode::BAD_GATEWAY, None);

    assert_eq!(asked.wait(BACKOFF), Some(Duration::from_secs(3)));
    assert_eq!(unasked.wait(BACKOFF), BACKOFF);
}

#[test]
fn should_stop_once_the_attempts_are_spent_or_the_wait_passes_the_ceiling() {
    let asked = status(StatusCode::TOO_MANY_REQUESTS, Some(Duration::from_secs(3)));
    let too_long = status(
        StatusCode::TOO_MANY_REQUESTS,
        Some(WAIT_CEILING + Duration::from_secs(1)),
    );
    let at_ceiling = status(StatusCode::TOO_MANY_REQUESTS, Some(WAIT_CEILING));

    assert_eq!(
        asked.wait(None),
        None,
        "no attempts left, whatever was asked"
    );
    assert_eq!(too_long.wait(BACKOFF), None);
    assert_eq!(at_ceiling.wait(BACKOFF), Some(WAIT_CEILING));
}

// rig reads the refusal itself, so the last answer goes back as it arrived:
// its status, and with it the provider's own code in the body.
#[test]
fn should_hand_back_an_unanswered_turns_last_answer_as_it_arrived() {
    let refused = status(StatusCode::TOO_MANY_REQUESTS, None)
        .into_answer()
        .unwrap();

    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn should_read_retry_after_as_seconds_or_as_a_date() {
    let past = httpdate::fmt_http_date(SystemTime::UNIX_EPOCH);

    assert_eq!(retry_after(&header("1")), Some(Duration::from_secs(1)));
    assert_eq!(retry_after(&header(" 7 ")), Some(Duration::from_secs(7)));
    assert_eq!(retry_after(&header(&past)), Some(Duration::ZERO));
    assert_eq!(retry_after(&header("soon")), None);
    assert_eq!(retry_after(&HeaderMap::new()), None);
}

#[test]
fn should_read_a_future_date_as_the_wait_until_it() {
    let later = SystemTime::now() + Duration::from_secs(30);

    let wait = retry_after(&header(&httpdate::fmt_http_date(later))).unwrap();

    assert!(
        wait > Duration::from_secs(25) && wait <= Duration::from_secs(30),
        "{wait:?}"
    );
}
