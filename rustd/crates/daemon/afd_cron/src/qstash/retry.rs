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
//! is asserted against [`SYNC_LEASE`].

use std::time::Duration;

use backon::{ExponentialBuilder, Retryable as _};
use reqwest::StatusCode;

use crate::error::{Error, ErrorKind, Result};
use crate::store::SYNC_LEASE;

/// How many times one call is sent before its failure is recorded on the row.
const ATTEMPTS: usize = 3;

/// The first wait, and the base the second doubles from.
pub(super) const FIRST_DELAY: Duration = Duration::from_millis(200);

/// The ceiling one wait may reach before jitter.
const MAX_DELAY: Duration = Duration::from_millis(800);

/// The longest one attempt may take, from connecting to the answer's last byte.
pub(super) const ATTEMPT_TIMEOUT: Duration = Duration::from_millis(2500);

/// The longest a retried call may take: the bound one attempt had before calls
/// were retried, and under the CLI's 15-second request timeout, so a caller
/// gets the row's "saved, not live" answer instead of its own timeout.
const CALL_BUDGET: Duration = Duration::from_secs(10);

/// The longest the waits between attempts add up to: each delay doubles from
/// `FIRST_DELAY` up to `MAX_DELAY`, and jitter adds up to one more of it.
pub(super) const fn longest_waits() -> u128 {
    let (mut total, mut delay, mut retry) = (0, FIRST_DELAY.as_millis(), 1);
    while retry < ATTEMPTS {
        total += 2 * delay;
        delay = if 2 * delay < MAX_DELAY.as_millis() {
            2 * delay
        } else {
            MAX_DELAY.as_millis()
        };
        retry += 1;
    }
    total
}

// Every attempt timing out, with every wait at its longest, still answers
// inside the call budget, and the budget inside the fence the syncer holds.
const _: () = assert!(
    ATTEMPT_TIMEOUT.as_millis() * ATTEMPTS as u128 + longest_waits() <= CALL_BUDGET.as_millis()
        && CALL_BUDGET.as_millis() < SYNC_LEASE.as_millis(),
    "every attempt and wait must end inside the call budget, and the budget inside the sync fence"
);

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
pub(super) async fn send<T, F, Fut>(attempts: Attempts, schedule_id: &str, mut call: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if attempts == Attempts::Once {
        return call().await;
    }
    let mut repeat = 0;
    call.retry(schedule())
        .when(heals)
        .notify(|error, delay| {
            repeat += 1;
            log(schedule_id, repeat, error, delay);
        })
        .await
}

/// The waits one call's attempts are spread over.
///
/// `max_times` counts RETRIES, one fewer than the attempts, derived from
/// [`ATTEMPTS`] so the two cannot drift.
pub(super) const fn schedule() -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_min_delay(FIRST_DELAY)
        .with_max_delay(MAX_DELAY)
        .with_max_times(ATTEMPTS - 1)
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
    tracing::debug!(
        schedule_id,
        attempt,
        delay_ms,
        error_code = %error_code,
        event = EVENT_RETRYING
    );
}

/// A scheduler call that failed in a way that can heal, about to be sent again.
const EVENT_RETRYING: &str = "schedule_upstream_retrying";
