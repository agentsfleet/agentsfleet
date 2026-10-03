//! Bounded retry of one turn's request, before any of its stream is read.
//!
//! `backon` runs the loop, as it does for the supervisor's calls to the
//! daemon; what is decided here is which answers are worth another send and how
//! long to wait. A 429 or a 5xx is retried, honouring `Retry-After` in either
//! of its forms, and so is a send that could not connect or timed out; a 4xx is
//! a refusal no retry changes (RULE ECL). Nothing is retried once a stream has
//! started, since its chunks have already gone out live. A wait the provider
//! asks for past [`WAIT_CEILING`] is not waited: the lease would spend its time
//! asleep, so the turn ends with the status instead.

use std::time::{Duration, SystemTime};

use backon::{ExponentialBuilder, Retryable as _};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::{RequestBuilder, Response, StatusCode};

use crate::error::{Error, Result};

/// Sends per turn, the first included.
pub(crate) const ATTEMPTS: usize = 3;
/// The wait before the second send when the provider names none; it grows
/// for each send after, jittered.
const BACKOFF: Duration = Duration::from_secs(1);
/// The longest wait honoured; a provider asking for more ends the turn.
pub(crate) const WAIT_CEILING: Duration = Duration::from_secs(60);

/// One retry about to happen, for the caller's log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Retrying {
    /// The send that failed, from 1.
    pub(crate) attempt: u32,
    /// The status it failed with; none for a send that never got one.
    pub(crate) status: Option<u16>,
    /// How long until the next send.
    pub(crate) wait: Duration,
}

/// Why one send did not answer.
#[derive(Debug)]
enum Unanswered {
    /// The provider answered with a status other than success.
    Status {
        status: StatusCode,
        retry_after: Option<Duration>,
    },
    /// The send never got a status.
    Transport(reqwest::Error),
}

impl Unanswered {
    /// Whether another send could be answered differently.
    fn retryable(&self) -> bool {
        match self {
            Self::Status { status, .. } => {
                *status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
            }
            Self::Transport(failure) => failure.is_connect() || failure.is_timeout(),
        }
    }

    /// The wait before the next send: the provider's, or else the backoff's.
    /// `None` stops: the attempts are spent, or the provider asked for more
    /// than [`WAIT_CEILING`].
    fn wait(&self, backoff: Option<Duration>) -> Option<Duration> {
        let backoff = backoff?;
        let wait = match self {
            Self::Status { retry_after, .. } => retry_after.unwrap_or(backoff),
            Self::Transport(_) => backoff,
        };
        (wait <= WAIT_CEILING).then_some(wait)
    }

    fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(status.as_u16()),
            Self::Transport(_) => None,
        }
    }

    fn into_error(self) -> Error {
        match self {
            Self::Status { status, .. } => Error::refused(status.as_u16()),
            Self::Transport(failure) => Error::lost(failure),
        }
    }
}

/// Sends the request `build` makes until it is answered, refused, or out of
/// attempts. `retrying` hears of each retry before its wait.
///
/// # Errors
/// A refusal with the last status, or a lost connection with the last
/// transport failure.
pub(crate) async fn send(
    build: impl Fn() -> RequestBuilder,
    mut retrying: impl FnMut(Retrying),
) -> Result<Response> {
    let policy = ExponentialBuilder::default()
        .with_min_delay(BACKOFF)
        .with_max_times(ATTEMPTS - 1)
        .with_jitter();
    let mut attempt = 0;
    (|| async { answered(build().send().await) })
        .retry(policy)
        .when(Unanswered::retryable)
        .adjust(Unanswered::wait)
        .notify(|unanswered, wait| {
            attempt += 1;
            let status = unanswered.status();
            retrying(Retrying {
                attempt,
                status,
                wait,
            });
        })
        .await
        .map_err(Unanswered::into_error)
}

/// One send's result as an answer, or why it was not one.
fn answered(sent: reqwest::Result<Response>) -> std::result::Result<Response, Unanswered> {
    let response = sent.map_err(Unanswered::Transport)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let retry_after = retry_after(response.headers());
    Err(Unanswered::Status {
        status,
        retry_after,
    })
}

/// The wait `Retry-After` names: whole seconds, or an HTTP date from now.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .ok()
        .or_else(|| {
            let at = httpdate::parse_http_date(value).ok()?;
            Some(at.duration_since(SystemTime::now()).unwrap_or_default())
        })
}

#[cfg(test)]
#[path = "retry/tests.rs"]
mod tests;
