//! The supervisor's bounds the runner's file chooses: its calls to the
//! daemon, the live tail, decision calls, what every sandbox enforces and how
//! long a resumed park has to answer.
//!
//! Every default is the constant the supervisor ran with before the file
//! could choose it. The binary checks the file; this crate takes the values.

use std::time::Duration;

use af_providers::decision::DecisionTimeouts;
use afr_engine::Limits;

use crate::activity::{MAX_BATCH_BYTES, MAX_BATCHES_HELD};
use crate::client::CALL_TIMEOUT;

/// The shortest a chosen call timeout may be, in seconds.
///
/// A call timeout must outlast the daemon's message deadline: shorter, a
/// slow channel reads as a daemon that never answered, and the model posts
/// its line again.
pub const CALL_TIMEOUT_MIN_SECONDS: u64 =
    af_wire::message_verb::MESSAGE_DELIVERY_DEADLINE.as_secs() + 1;

/// How long a resumed parked sandbox's executor has to answer before the
/// park is given up for a fresh sandbox, by default.
///
/// Two seconds is Indy's number (Indy, Nov 27, 2026: "park resume wait 2 s"). The runner's
/// `[sandbox] park_resume_timeout_seconds` chooses it.
pub const PARK_RESUME_TIMEOUT: Duration = Duration::from_secs(2);

/// The supervisor's chosen bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    /// How long one call to the daemon may take, end to end.
    pub call_timeout: Duration,
    /// Live-tail batches queued or in flight at once; at least two, one in
    /// flight and one queued.
    pub activity_batches_held: usize,
    /// The most frame bytes one live-tail batch carries.
    pub activity_batch_bytes: usize,
    /// How long a decision call may take to connect, and to finish.
    pub decision: DecisionTimeouts,
    /// What every run's sandbox enforces.
    pub limits: Limits,
    /// How long a resumed parked sandbox's executor has to answer.
    pub park_resume_timeout: Duration,
}

impl Tuning {
    /// The bounds the supervisor held before the file could choose them.
    pub const DEFAULT: Self = Self {
        call_timeout: CALL_TIMEOUT,
        activity_batches_held: MAX_BATCHES_HELD,
        activity_batch_bytes: MAX_BATCH_BYTES,
        decision: DecisionTimeouts::DEFAULT,
        limits: Limits::DEFAULT,
        park_resume_timeout: PARK_RESUME_TIMEOUT,
    };
}
