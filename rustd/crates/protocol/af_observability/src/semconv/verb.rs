//! The daemon verb spans' vocabulary: each verb's span name, its stage keys,
//! and every key a verb span may carry.

use super::{ATTR_AGENT_ID, ATTR_TURN_ID};

/// The span one steer leaves: admission to the announce.
pub const SPAN_FLEET_STEER: &str = "fleet.steer";

/// The span one assignment that selected work leaves: claim to the stream write.
pub const SPAN_FLEET_ASSIGN: &str = "fleet.assign";

/// The span one terminal report leaves: the run read to the acknowledgement.
pub const SPAN_FLEET_REPORT: &str = "fleet.report";

/// The span one fallback memory load leaves.
pub const SPAN_FLEET_LOAD: &str = "fleet.load";

/// The span one memory capture leaves.
pub const SPAN_FLEET_CAPTURE: &str = "fleet.capture";

/// The steer verb's `admit` stage, in whole milliseconds.
pub const ATTR_STEER_ADMIT_MS: &str = "agentsfleet.steer.admit_ms";

/// The steer verb's `announce` stage, in whole milliseconds.
pub const ATTR_STEER_ANNOUNCE_MS: &str = "agentsfleet.steer.announce_ms";

/// The assignment verb's `claim` stage, in whole milliseconds.
pub const ATTR_ASSIGN_CLAIM_MS: &str = "agentsfleet.assign.claim_ms";

/// The assignment verb's `admit` stage, in whole milliseconds.
pub const ATTR_ASSIGN_ADMIT_MS: &str = "agentsfleet.assign.admit_ms";

/// The assignment verb's `issue` stage, in whole milliseconds.
pub const ATTR_ASSIGN_ISSUE_MS: &str = "agentsfleet.assign.issue_ms";

/// The assignment verb's `encode` stage, in whole milliseconds.
pub const ATTR_ASSIGN_ENCODE_MS: &str = "agentsfleet.assign.encode_ms";

/// The assignment verb's `send` stage, in whole milliseconds.
pub const ATTR_ASSIGN_SEND_MS: &str = "agentsfleet.assign.send_ms";

/// The report verb's `load` stage, in whole milliseconds.
pub const ATTR_REPORT_LOAD_MS: &str = "agentsfleet.report.load_ms";

/// The report verb's `prepare` stage, in whole milliseconds.
pub const ATTR_REPORT_PREPARE_MS: &str = "agentsfleet.report.prepare_ms";

/// The report verb's `capture` stage, in whole milliseconds.
pub const ATTR_REPORT_CAPTURE_MS: &str = "agentsfleet.report.capture_ms";

/// The report verb's `commit` stage, in whole milliseconds.
pub const ATTR_REPORT_COMMIT_MS: &str = "agentsfleet.report.commit_ms";

/// The report verb's `announce` stage, in whole milliseconds.
pub const ATTR_REPORT_ANNOUNCE_MS: &str = "agentsfleet.report.announce_ms";

/// The report verb's `ack` stage, in whole milliseconds.
pub const ATTR_REPORT_ACK_MS: &str = "agentsfleet.report.ack_ms";

/// The load verb's `fence` stage, in whole milliseconds.
pub const ATTR_LOAD_FENCE_MS: &str = "agentsfleet.load.fence_ms";

/// The load verb's `memory_read` stage, in whole milliseconds.
pub const ATTR_LOAD_MEMORY_READ_MS: &str = "agentsfleet.load.memory_read_ms";

/// The capture verb's `fence` stage, in whole milliseconds.
pub const ATTR_CAPTURE_FENCE_MS: &str = "agentsfleet.capture.fence_ms";

/// The capture verb's `memory_write` stage, in whole milliseconds.
pub const ATTR_CAPTURE_MEMORY_WRITE_MS: &str = "agentsfleet.capture.memory_write_ms";

/// Every key a daemon verb span may carry: its identity, then each verb's
/// stages. A verb span writes nothing outside this list.
pub const VERB_SPAN_KEYS: &[&str] = &[
    ATTR_TURN_ID,
    ATTR_AGENT_ID,
    ATTR_STEER_ADMIT_MS,
    ATTR_STEER_ANNOUNCE_MS,
    ATTR_ASSIGN_CLAIM_MS,
    ATTR_ASSIGN_ADMIT_MS,
    ATTR_ASSIGN_ISSUE_MS,
    ATTR_ASSIGN_ENCODE_MS,
    ATTR_ASSIGN_SEND_MS,
    ATTR_REPORT_LOAD_MS,
    ATTR_REPORT_PREPARE_MS,
    ATTR_REPORT_CAPTURE_MS,
    ATTR_REPORT_COMMIT_MS,
    ATTR_REPORT_ANNOUNCE_MS,
    ATTR_REPORT_ACK_MS,
    ATTR_LOAD_FENCE_MS,
    ATTR_LOAD_MEMORY_READ_MS,
    ATTR_CAPTURE_FENCE_MS,
    ATTR_CAPTURE_MEMORY_WRITE_MS,
];
