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

use backon::ExponentialBuilder;
use reqwest::StatusCode;

use crate::error::{Error, ErrorKind};
use crate::store::SYNC_LEASE;

/// How many times one call is sent before its failure is recorded on the row.
const ATTEMPTS: usize = 3;

/// The first wait, and the base the second doubles from.
const FIRST_DELAY: Duration = Duration::from_millis(200);

/// The ceiling one wait may reach before jitter.
const MAX_DELAY: Duration = Duration::from_millis(800);

/// The longest one attempt may take, from connecting to the answer's last
/// byte. Five seconds is the save path's own wait for a schedule sync, so the
/// first attempt alone fits inside it.
pub(super) const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);

// Jitter adds up to one more delay to each wait, so a wait stays under twice
// `MAX_DELAY`. Every attempt timing out, with every wait at its longest, still
// ends while the fence is held.
const _: () = assert!(
    ATTEMPT_TIMEOUT.as_millis() * ATTEMPTS as u128
        + 2 * MAX_DELAY.as_millis() * (ATTEMPTS as u128 - 1)
        < SYNC_LEASE.as_millis(),
    "every attempt and every jittered wait must end inside the sync fence"
);

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

/// Records a repeat before its wait.
pub(super) fn log(error: &Error, delay: Duration) {
    let delay_ms = delay.as_millis();
    tracing::debug!(
        delay_ms,
        error_code = %error.code(),
        event = EVENT_RETRYING
    );
}

/// A scheduler call that failed in a way that can heal, about to be sent again.
const EVENT_RETRYING: &str = "schedule_upstream_retrying";
