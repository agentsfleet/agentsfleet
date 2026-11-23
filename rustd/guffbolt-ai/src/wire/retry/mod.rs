//! Bounded retry of one turn's request, before any of its stream is read.
//!
//! `backon` runs the loop; what is decided here is which answers are worth
//! another send and how long to wait. A 429 or a 5xx is retried, honouring
//! `Retry-After` in either of its forms, and so is a send that was not
//! answered, unless the embedder's guard refused its destination: that is
//! refused the same way every time. A 4xx is a refusal no retry changes.
//! Nothing is retried once a stream has started, since its chunks have
//! already gone out live. A wait the provider asks for past the wait ceiling
//! is not waited: the run would spend its time asleep, so the turn ends with
//! the status instead.
//!
//! The last answer, a refusal included, is handed back as it arrived, so the
//! wire library reading it keeps its status, its headers and the provider's
//! own error code.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use backon::{ExponentialBuilder, Retryable as _};
use bytes::Bytes;
use http::header::{HeaderMap, RETRY_AFTER};
use http::{Response, StatusCode};

use super::observe::Observer;
use super::transport::{Body, Send, SendError, Sent};

/// Three sends of one request, and three openings of one turn.
const ATTEMPTS: usize = 3;
/// The first backoff, before jitter.
const BACKOFF: Duration = Duration::from_secs(1);
/// The longest `Retry-After` waited: 60 s, the reference agent library's
/// longest retry delay.
const WAIT_CEILING: Duration = Duration::from_secs(60);

/// How often a turn's request is sent and its stream opened. Totals include
/// the first send or opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    pub(crate) attempts: usize,
    pub(crate) openings: usize,
    pub(crate) backoff: Duration,
    pub(crate) wait_ceiling: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Retry {
    /// Three sends and three openings, a 1 s first backoff, and no wait past
    /// 60 s.
    pub const DEFAULT: Self = Self {
        attempts: ATTEMPTS,
        openings: ATTEMPTS,
        backoff: BACKOFF,
        wait_ceiling: WAIT_CEILING,
    };

    /// The same retry, sending one request at most `attempts` times before
    /// any of its body is read.
    #[must_use]
    pub const fn with_attempts(mut self, attempts: usize) -> Self {
        self.attempts = attempts;
        self
    }

    /// The same retry, opening a turn at most `openings` times; only before
    /// visible output or known spend.
    #[must_use]
    pub const fn with_openings(mut self, openings: usize) -> Self {
        self.openings = openings;
        self
    }

    /// The same retry, its exponential backoff starting at `backoff`.
    #[must_use]
    pub const fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    /// The same retry, a provider asking for a longer wait than
    /// `wait_ceiling` ending the attempt instead.
    #[must_use]
    pub const fn with_wait_ceiling(mut self, wait_ceiling: Duration) -> Self {
        self.wait_ceiling = wait_ceiling;
        self
    }

    /// The openings a turn may have.
    pub(crate) const fn openings(self) -> usize {
        self.openings
    }
}

/// One retry about to happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retrying {
    /// The send that failed, from 1.
    pub attempt: u32,
    /// The status it failed with; none for a send that never got one.
    pub status: Option<u16>,
    /// How long until the next send.
    pub wait: Duration,
}

/// A [`Send`] that sends again under a [`Retry`].
pub(crate) struct Retried {
    inner: Arc<dyn Send>,
    retry: Retry,
    observer: Option<Arc<dyn Observer>>,
}

impl Retried {
    pub(crate) fn new(
        inner: Arc<dyn Send>,
        retry: Retry,
        observer: Option<Arc<dyn Observer>>,
    ) -> Self {
        Self {
            inner,
            retry,
            observer,
        }
    }
}

impl Send for Retried {
    fn send(&self, request: http::Request<Bytes>) -> Sent {
        let inner = Arc::clone(&self.inner);
        let observer = self.observer.clone();
        let retry = self.retry;
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let build = || http::Request::from_parts(parts.clone(), body.clone());
            send(&*inner, build, observer.as_deref(), retry).await
        })
    }
}

/// Why one send did not answer.
enum Unanswered {
    /// The provider answered with a status other than success.
    Status {
        /// Boxed: a refusal is the rare arm, and the retry loop moves this
        /// value on every attempt.
        response: Box<Response<Body>>,
        retry_after: Option<Duration>,
    },
    /// The send never got a status.
    Transport(SendError),
}

impl Unanswered {
    /// Whether another send could be answered differently.
    fn retryable(&self) -> bool {
        match self {
            Self::Status { response, .. } => {
                let status = response.status();
                status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
            }
            Self::Transport(failure) => failure.is_unanswered(),
        }
    }

    /// The wait before the next send: the provider's, or else the backoff's.
    /// `None` stops: the attempts are spent, or the provider asked for more
    /// than the wait ceiling.
    fn wait(&self, backoff: Option<Duration>, ceiling: Duration) -> Option<Duration> {
        let backoff = backoff?;
        let wait = match self {
            Self::Status { retry_after, .. } => retry_after.unwrap_or(backoff),
            Self::Transport(_) => backoff,
        };
        (wait <= ceiling).then_some(wait)
    }

    fn status(&self) -> Option<u16> {
        match self {
            Self::Status { response, .. } => Some(response.status().as_u16()),
            Self::Transport(_) => None,
        }
    }

    /// The answer the caller reads: the last response, refusal or not, or
    /// the transport's failure.
    fn into_answer(self) -> Result<Response<Body>, SendError> {
        match self {
            Self::Status { response, .. } => Ok(*response),
            Self::Transport(failure) => Err(failure),
        }
    }
}

/// Sends the request `build` makes until it is answered, refused, or out of
/// attempts. `observer` hears of each retry before its wait.
///
/// # Errors
/// The last transport failure, for a send that never got a status.
async fn send(
    inner: &dyn Send,
    build: impl Fn() -> http::Request<Bytes> + Sync,
    observer: Option<&dyn Observer>,
    settings: Retry,
) -> Result<Response<Body>, SendError> {
    let policy = ExponentialBuilder::default()
        .with_min_delay(settings.backoff)
        .with_max_times(settings.attempts.saturating_sub(1))
        .with_jitter();
    let mut attempt = 0;
    let sent = (async || answered(inner.send(build()).await))
        .retry(policy)
        .when(Unanswered::retryable)
        .adjust(|failure, delay| failure.wait(delay, settings.wait_ceiling))
        .notify(|unanswered, wait| {
            attempt += 1;
            let status = unanswered.status();
            if let Some(observer) = observer {
                observer.retrying(Retrying {
                    attempt,
                    status,
                    wait,
                });
            }
        })
        .await;
    sent.or_else(Unanswered::into_answer)
}

/// One send's result as an answer, or why it was not one.
fn answered(sent: Result<Response<Body>, SendError>) -> Result<Response<Body>, Unanswered> {
    let response = sent.map_err(Unanswered::Transport)?;
    if response.status().is_success() {
        return Ok(response);
    }
    let retry_after = retry_after(response.headers());
    Err(Unanswered::Status {
        response: Box::new(response),
        retry_after,
    })
}

/// The wait a response's `Retry-After` names: whole seconds, or an HTTP date
/// from now. A value in neither form names none.
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "the HTTP date parses to a SystemTime and the wait is a Duration between two SystemTimes; this crate links no agentsfleet clock"
)]
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
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
mod tests;
