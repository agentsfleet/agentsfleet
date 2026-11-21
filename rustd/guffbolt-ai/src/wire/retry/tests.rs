#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests build a SystemTime-based HTTP date for retry_after; it is no knob"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures_util::StreamExt as _;
use http::StatusCode;
use http::header::{HeaderMap, HeaderValue, RETRY_AFTER};

use super::{Retried, Retry, Unanswered, retry_after};
use crate::wire::recorder::{Recorder, Seen};
use crate::wire::transport::{Body, Send, SendError, Sent};

const WAIT_CEILING: Duration = Retry::DEFAULT.wait_ceiling;

const BACKOFF: Option<Duration> = Some(Duration::from_secs(1));

/// An answer with `code` and an empty body, asking for `retry_after`.
fn status(code: StatusCode, retry_after: Option<Duration>) -> Unanswered {
    let body: Body = futures_util::stream::empty().boxed();
    let answer = http::Response::builder().status(code).body(body).unwrap();
    Unanswered::Status {
        response: Box::new(answer),
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

#[cfg(feature = "reqwest")]
#[tokio::test]
async fn should_retry_a_send_that_could_not_connect() {
    // Port 1 is reserved and nothing listens on it, so the connect is refused.
    let request = http::Request::builder()
        .uri("http://127.0.0.1:1/")
        .body(Bytes::new())
        .unwrap();
    let refused = crate::wire::transport::Reqwest::new()
        .unwrap()
        .send(request)
        .await
        .err()
        .unwrap();

    let unanswered = Unanswered::Transport(refused);

    assert!(unanswered.retryable());
    assert_eq!(unanswered.status(), None);
    assert_eq!(
        unanswered.wait(BACKOFF, WAIT_CEILING),
        BACKOFF,
        "a send that never got a status waits the backoff"
    );
    assert!(
        unanswered.into_answer().err().unwrap().is_unanswered(),
        "the transport's own failure, for rig to read"
    );
}

#[test]
fn should_not_retry_a_destination_the_guard_refused() {
    let refused = Unanswered::Transport(SendError::Refused(Box::new(std::io::Error::other(
        "blocked",
    ))));

    assert!(!refused.retryable());
}

#[test]
fn should_wait_what_the_provider_asks_and_else_the_backoff() {
    let asked = status(StatusCode::TOO_MANY_REQUESTS, Some(Duration::from_secs(3)));
    let unasked = status(StatusCode::BAD_GATEWAY, None);

    assert_eq!(
        asked.wait(BACKOFF, WAIT_CEILING),
        Some(Duration::from_secs(3))
    );
    assert_eq!(unasked.wait(BACKOFF, WAIT_CEILING), BACKOFF);
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
        asked.wait(None, WAIT_CEILING),
        None,
        "no attempts left, whatever was asked"
    );
    assert_eq!(too_long.wait(BACKOFF, WAIT_CEILING), None);
    assert_eq!(at_ceiling.wait(BACKOFF, WAIT_CEILING), Some(WAIT_CEILING));
}

// rig reads the refusal itself, so the last answer goes back as it arrived:
// its status, and with it the provider's own code in the body.
#[test]
fn should_hand_back_an_unanswered_turns_last_answer_as_it_arrived() {
    let refused = status(StatusCode::TOO_MANY_REQUESTS, None)
        .into_answer()
        .ok()
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

#[test]
fn the_default_retry_policy_is_the_existing_provider_behavior() {
    assert_eq!(Retry::default(), Retry::DEFAULT);
    assert_eq!(Retry::DEFAULT.attempts, 3);
    assert_eq!(Retry::DEFAULT.openings, 3);
    assert_eq!(Retry::DEFAULT.backoff, Duration::from_secs(1));
    assert_eq!(Retry::DEFAULT.wait_ceiling, Duration::from_secs(60));
}

/// A provider that answers every send with `status`, counting the sends.
struct Answering {
    status: StatusCode,
    sends: AtomicUsize,
}

impl Send for Answering {
    fn send(&self, _: http::Request<Bytes>) -> Sent {
        self.sends.fetch_add(1, Ordering::SeqCst);
        let status = self.status;
        Box::pin(async move {
            let body: Body = futures_util::stream::empty().boxed();
            Ok(http::Response::builder().status(status).body(body).unwrap())
        })
    }
}

#[tokio::test(start_paused = true)]
async fn configured_http_attempts_cap_physical_sends() {
    for attempts in [1, 2, 4] {
        let physical = Arc::new(Answering {
            status: StatusCode::SERVICE_UNAVAILABLE,
            sends: AtomicUsize::new(0),
        });
        let recorder = Recorder::new();
        let retry = Retry::DEFAULT
            .with_attempts(attempts)
            .with_backoff(Duration::from_millis(1));
        let retried = Retried::new(
            Arc::<Answering>::clone(&physical),
            retry,
            Some(Arc::<Recorder>::clone(&recorder)),
        );
        let response = retried
            .send(http::Request::new(Bytes::new()))
            .await
            .ok()
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(physical.sends.load(Ordering::SeqCst), attempts);
        let notified = recorder
            .seen()
            .into_iter()
            .filter(|seen| matches!(seen, Seen::Retrying(retry) if retry.status == Some(503)))
            .count();
        assert_eq!(notified, attempts - 1);
    }
}

// Value: a single-attempt send never retries a possibly charged rejection.
#[tokio::test]
async fn a_single_attempt_send_returns_the_first_failure_without_retrying() {
    let physical = Arc::new(Answering {
        status: StatusCode::SERVICE_UNAVAILABLE,
        sends: AtomicUsize::new(0),
    });
    let retried = Retried::new(
        Arc::<Answering>::clone(&physical),
        Retry::DEFAULT.with_attempts(1),
        None,
    );
    let response = retried
        .send(http::Request::new(Bytes::new()))
        .await
        .ok()
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(physical.sends.load(Ordering::SeqCst), 1);
}
