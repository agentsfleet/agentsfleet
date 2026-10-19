//! Native lease behavior grouped by completion, renewal, and startup failure.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use afd_core::id::Uuid7;
use bytes::Bytes;
use tokio::time::Instant;

use super::ACTIVITY_DRAIN_WAIT;
use crate::client::{Call, Verb};
use crate::egress::Egress;
use crate::error;
use crate::report_spool::ReportSpool;
use crate::test_support::{
    Answer, Behaviour, FAILURE_REASON, FENCING, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME,
    PROCESSED, RENEWAL_TERMINATE, RUNNER_CRASH, Rig, STARTUP_POSTURE, daemon, lease, position,
    reported,
};
use crate::turns::FleetTurns;
use afd_core::timing::RENEWAL_TICK;

/// The metered fields a renewal and a report carry, by their wire names.
const TOKENS: &str = "tokens";
const INPUT_TOKENS: &str = "input_tokens";
const CACHED_INPUT_TOKENS: &str = "cached_input_tokens";
const OUTPUT_TOKENS: &str = "output_tokens";
const REQUESTS: &str = "requests";

/// One answer the test daemon gives before its defaults.
type Special = fn(&Call) -> Option<Answer>;

fn rig(special: Special, engine: FakeEngine, behaviour: Behaviour) -> Rig {
    Rig::new(daemon(special), engine, FakeAgent::new(behaviour))
}

fn healthy(_call: &Call) -> Option<Answer> {
    None
}

fn renewal_lost(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
}

mod completion;
mod failures;
mod renewal;
