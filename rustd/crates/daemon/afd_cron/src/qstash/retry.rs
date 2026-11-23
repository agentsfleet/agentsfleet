//! Which scheduler failures a management call is sent again for, and how often.
//!
//! # The schedule is `backon`'s, as `afd_outbound`'s delivery schedule is
//!
//! [`ExponentialBuilder`] carries the floor, the ceiling, the attempt count and
//! the jitter. The curve is the one `afd_outbound/src/retry.rs` uses, and that
//! module's note is why the delays are jittered.
//!
//! # What is sent again
//!
//! A scheduler that was never reached, a 429 and a 5xx can heal on their own.
//! Every call here is safe to send twice: a push names its schedule by an id
//! this daemon chose, so a repeat overwrites rather than adds, and removing a
//! schedule that is already gone answers 404, which is success. Any other 4xx
//! is a request to fix, and an unreadable answer reads the same way again, so
//! neither is repeated.
//!
//! # Why every attempt carries its own bound
//!
//! These calls run while the syncer holds the schedule's fence. Three attempts
//! at the client's ten seconds would outlast that fence and let a second syncer
//! push beside this one, so each attempt is bounded here and the whole budget
//! is asserted against [`SYNC_RUN`].

// # The credentials are read before the first attempt
//
// One read per call, ahead of the loop. The bag is one pooled statement,
// bounded by the pool's own acquire timeout as every other statement of the
// sync path is, and a bag that cannot be read ends the call before any request
// is sent. A read inside each attempt would sit outside [`ATTEMPT_TIMEOUT`] and
// would retry a key fault no wait can repair.

use std::time::Duration;

use backon::{ExponentialBuilder, Retryable as _};
use reqwest::StatusCode;

use crate::error::{Error, ErrorKind, Result};
use crate::store::SYNC_RUN;

/// How many times one call is sent before its failure is recorded on the row,
/// when nothing configures it.
pub const ATTEMPTS: usize = 3;

/// The first wait, and the base the second doubles from.
pub(super) const FIRST_DELAY: Duration = Duration::from_millis(200);

/// The ceiling one wait may reach before jitter, by default.
pub const MAX_DELAY: Duration = Duration::from_millis(800);

/// The longest one attempt may take, from connecting to the answer's last
/// byte, by default.
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_millis(2500);

/// The longest a retried call may take, by default.
///
/// The bound one attempt had before calls were retried, and under the CLI's
/// 15-second request timeout, so a caller gets the row's "saved, not live"
/// answer instead of its own timeout.
pub const CALL_BUDGET: Duration = Duration::from_secs(10);

/// The longest the waits between `attempts` add up to: each delay doubles from
/// `FIRST_DELAY` up to `max_delay`, and jitter adds up to one more of it.
pub(super) const fn longest_waits(attempts: usize, max_delay: Duration) -> u128 {
    let (mut total, mut delay, mut retry) = (0, FIRST_DELAY.as_millis(), 1);
    while retry < attempts {
        total += 2 * delay;
        delay = if 2 * delay < max_delay.as_millis() {
            2 * delay
        } else {
            max_delay.as_millis()
        };
        retry += 1;
    }
    total
}

/// Whether every attempt timing out, with every wait at its longest, still
/// answers inside `call_budget`, and the budget inside the fence the syncer
/// holds.
const fn fits(
    attempts: usize,
    attempt_timeout: Duration,
    max_delay: Duration,
    call_budget: Duration,
) -> bool {
    attempt_timeout.as_millis() * attempts as u128 + longest_waits(attempts, max_delay)
        <= call_budget.as_millis()
        && call_budget.as_millis() < SYNC_RUN.as_millis()
}

const _: () = assert!(
    fits(ATTEMPTS, ATTEMPT_TIMEOUT, MAX_DELAY, CALL_BUDGET),
    "every attempt and wait must end inside the call budget, and the budget inside the sync fence"
);

/// How often one scheduler call is sent, how long each attempt and wait may
/// take, proved to end inside its call budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    attempts: usize,
    attempt_timeout: Duration,
    max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl RetryPolicy {
    /// The compiled policy: [`ATTEMPTS`] tries of [`ATTEMPT_TIMEOUT`], waits
    /// up to [`MAX_DELAY`].
    pub const DEFAULT: Self = Self {
        attempts: ATTEMPTS,
        attempt_timeout: ATTEMPT_TIMEOUT,
        max_delay: MAX_DELAY,
    };

    /// A policy of `attempts` tries of at most `attempt_timeout` each, waiting
    /// at most `max_delay` between them.
    ///
    /// `None` when a try or a wait could outlast `call_budget`, when the budget
    /// could outlast the schedule's sync fence, when there is no try, or when
    /// `max_delay` is below the first wait.
    #[must_use]
    pub const fn new(
        attempts: usize,
        attempt_timeout: Duration,
        max_delay: Duration,
        call_budget: Duration,
    ) -> Option<Self> {
        if attempts == 0
            || max_delay.as_millis() < FIRST_DELAY.as_millis()
            || !fits(attempts, attempt_timeout, max_delay, call_budget)
        {
            return None;
        }
        Some(Self {
            attempts,
            attempt_timeout,
            max_delay,
        })
    }

    /// The longest one attempt may take.
    #[must_use]
    pub const fn attempt_timeout(&self) -> Duration {
        self.attempt_timeout
    }
}

/// How many times a scheduler call is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempts {
    /// Sent once. For a caller syncing many schedules under one deadline, where
    /// repeating one would leave the rest unsynced.
    Once,
    /// Sent again on a failure that can heal, on [`schedule`].
    Retried,
}

/// Sends `call` once, or again while it fails in a way that [`heals`],
/// recording each repeat under `schedule_id`.
pub(super) async fn send<T, F, Fut>(
    attempts: Attempts,
    policy: RetryPolicy,
    schedule_id: &str,
    mut call: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if attempts == Attempts::Once {
        return call().await;
    }
    let mut repeat = 0;
    call.retry(schedule(policy))
        .when(heals)
        .notify(|error, delay| {
            repeat += 1;
            log(schedule_id, repeat, error, delay);
        })
        .await
}

/// The waits one call's attempts are spread over.
///
/// `max_times` counts RETRIES, one fewer than the attempts, derived from the
/// policy's attempts so the two cannot drift.
pub(super) const fn schedule(policy: RetryPolicy) -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_min_delay(FIRST_DELAY)
        .with_max_delay(policy.max_delay)
        .with_max_times(policy.attempts.saturating_sub(1))
        .with_jitter()
}

/// Whether sending the same call again could get a different answer.
pub(super) fn heals(error: &Error) -> bool {
    if let ErrorKind::UpstreamRefused { status } = error.kind() {
        return StatusCode::from_u16(*status)
            .is_ok_and(|code| code == StatusCode::TOO_MANY_REQUESTS || code.is_server_error());
    }
    matches!(error.kind(), ErrorKind::UpstreamUnreachable { .. })
}

/// Records a repeat before its wait: which schedule, which repeat, and why.
pub(super) fn log(schedule_id: &str, attempt: usize, error: &Error, delay: Duration) {
    let delay_ms = delay.as_millis();
    let error_code = error.code();
    {
        let event = &EVENT_RETRYING;
        tracing::debug!(schedule_id, attempt, delay_ms, %error_code, event);
    };
}

/// A scheduler call that failed in a way that can heal, about to be sent again.
const EVENT_RETRYING: &str = "schedule_upstream_retrying";
