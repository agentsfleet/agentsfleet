//! The daemon verb spans' vocabulary: each verb's span name, its stage keys,
//! and every key a verb span may carry.

use super::{ATTR_AGENT_ID, ATTR_EVENT_ID};

/// The span one steer leaves: admission to the announce.
pub const SPAN_FLEET_STEER: &str = "fleet.steer";

/// The span one offer that selected work leaves: claim to the stream write.
pub const SPAN_FLEET_OFFER: &str = "fleet.offer";

/// The span one terminal report leaves: the lease read to the acknowledgement.
pub const SPAN_FLEET_REPORT: &str = "fleet.report";

/// The span one fallback memory hydrate leaves.
pub const SPAN_FLEET_HYDRATE: &str = "fleet.hydrate";

/// The span one memory capture leaves.
pub const SPAN_FLEET_CAPTURE: &str = "fleet.capture";

/// The steer verb's `admit` stage, in whole milliseconds.
pub const ATTR_STEER_ADMIT_MS: &str = "agentsfleet.steer.admit_ms";

/// The steer verb's `announce` stage, in whole milliseconds.
pub const ATTR_STEER_ANNOUNCE_MS: &str = "agentsfleet.steer.announce_ms";

/// The offer verb's `claim` stage, in whole milliseconds.
pub const ATTR_OFFER_CLAIM_MS: &str = "agentsfleet.offer.claim_ms";

/// The offer verb's `admit` stage, in whole milliseconds.
pub const ATTR_OFFER_ADMIT_MS: &str = "agentsfleet.offer.admit_ms";

/// The offer verb's `issue` stage, in whole milliseconds.
pub const ATTR_OFFER_ISSUE_MS: &str = "agentsfleet.offer.issue_ms";

/// The offer verb's `encode` stage, in whole milliseconds.
pub const ATTR_OFFER_ENCODE_MS: &str = "agentsfleet.offer.encode_ms";

/// The offer verb's `send` stage, in whole milliseconds.
pub const ATTR_OFFER_SEND_MS: &str = "agentsfleet.offer.send_ms";

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

/// The hydrate verb's `fence` stage, in whole milliseconds.
pub const ATTR_HYDRATE_FENCE_MS: &str = "agentsfleet.hydrate.fence_ms";

/// The hydrate verb's `memory_read` stage, in whole milliseconds.
pub const ATTR_HYDRATE_MEMORY_READ_MS: &str = "agentsfleet.hydrate.memory_read_ms";

/// The capture verb's `fence` stage, in whole milliseconds.
pub const ATTR_CAPTURE_FENCE_MS: &str = "agentsfleet.capture.fence_ms";

/// The capture verb's `memory_write` stage, in whole milliseconds.
pub const ATTR_CAPTURE_MEMORY_WRITE_MS: &str = "agentsfleet.capture.memory_write_ms";

/// Every key a daemon verb span may carry: its identity, then each verb's
/// stages. A verb span writes nothing outside this list.
pub const VERB_SPAN_KEYS: &[&str] = &[
    ATTR_EVENT_ID,
    ATTR_AGENT_ID,
    ATTR_STEER_ADMIT_MS,
    ATTR_STEER_ANNOUNCE_MS,
    ATTR_OFFER_CLAIM_MS,
    ATTR_OFFER_ADMIT_MS,
    ATTR_OFFER_ISSUE_MS,
    ATTR_OFFER_ENCODE_MS,
    ATTR_OFFER_SEND_MS,
    ATTR_REPORT_LOAD_MS,
    ATTR_REPORT_PREPARE_MS,
    ATTR_REPORT_CAPTURE_MS,
    ATTR_REPORT_COMMIT_MS,
    ATTR_REPORT_ANNOUNCE_MS,
    ATTR_REPORT_ACK_MS,
    ATTR_HYDRATE_FENCE_MS,
    ATTR_HYDRATE_MEMORY_READ_MS,
    ATTR_CAPTURE_FENCE_MS,
    ATTR_CAPTURE_MEMORY_WRITE_MS,
];
