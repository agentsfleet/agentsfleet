//! Correlated answers preserve each service's complete response.

use std::borrow::Cow;
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use super::Lane;
use crate::fold::FoldResponse;
use crate::lease::TaggedLease;
use crate::report::{RenewResponse, ReportResponse};
use crate::runner::HeartbeatResponse;

/// A domain answer or a classified refusal, without optional success fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer<'a, T> {
    /// The full response, including any usage acknowledgement.
    Ok {
        /// Complete service response.
        response: T,
    },
    /// The caller uses this classification to retain or discard durable retries.
    Refused {
        /// Registered domain refusal code.
        #[serde(borrow)]
        error_code: Cow<'a, str>,
        /// Whether a later attempt can help.
        retryable: bool,
    },
}

/// An independent answer for each lease in a renewal batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenewalAnswer<'a> {
    /// Lease to which this answer and usage acknowledgement apply.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// Complete renewal response or refusal.
    #[serde(borrow)]
    pub answer: Answer<'a, RenewResponse>,
}

/// Every daemon message on the runner session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Down<'a> {
    /// The connection owns this runner and has rebuilt its live leases.
    Welcome {
        /// Identifier of the opening hello.
        request_id: NonZeroU64,
        /// Authoritative connection owner for secondary-lane binding.
        #[serde(borrow)]
        connection_id: Cow<'a, str>,
        /// Interval the runner must use for liveness reports.
        heartbeat_interval_ms: u32,
    },
    /// Work issued under the runner's available capacity.
    Offer {
        /// Fleet or authoring work, with its existing payload.
        #[serde(borrow)]
        lease: TaggedLease<'a>,
    },
    /// Current policy, capability verdict and held-sandbox releases.
    Heartbeat {
        /// Identifier of the heartbeat request.
        request_id: NonZeroU64,
        /// Existing complete heartbeat response or refusal.
        #[serde(borrow)]
        answer: Answer<'a, HeartbeatResponse<'a>>,
    },
    /// Application responsiveness acknowledgement.
    Pong {
        /// Identifier of the ping request.
        request_id: NonZeroU64,
    },
    /// One complete answer for every entry in the renewal batch.
    Renewed {
        /// Identifier of the renewal batch.
        request_id: NonZeroU64,
        /// Lease-specific deadlines, usage acknowledgements or refusals.
        #[serde(borrow)]
        leases: Vec<RenewalAnswer<'a>>,
    },
    /// The report settled, repeated its saved result, or was refused.
    ReportAck {
        /// Identifier of this transmission of the spooled report.
        request_id: NonZeroU64,
        /// Lease whose spool entry this answer can remove.
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        /// Full report response or classified refusal.
        #[serde(borrow)]
        answer: Answer<'a, ReportResponse>,
    },
    /// Pending input claimed under the requesting lease's fence.
    Folded {
        /// Identifier of the folds request.
        request_id: NonZeroU64,
        /// Lease whose input inbox receives this answer.
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        /// Claimed inputs or classified refusal.
        #[serde(borrow)]
        answer: Answer<'a, FoldResponse<'a>>,
    },
    /// Stop the named running lease.
    Cancel {
        /// Lease whose interrupt token must be cancelled.
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        /// Why this lease no longer has authority.
        #[serde(borrow)]
        error_code: Cow<'a, str>,
    },
    /// Reconnect after drain, or halt when superseded.
    GoAway {
        /// An absent code denotes ordinary graceful drain.
        #[serde(borrow)]
        error_code: Option<Cow<'a, str>>,
    },
}

impl Down<'_> {
    /// The response follows its request's lane; unsolicited control stays separate.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::ReportAck { .. } => Lane::Reports,
            Self::Offer { .. } | Self::Folded { .. } => Lane::Work,
            Self::Welcome { .. }
            | Self::Heartbeat { .. }
            | Self::Pong { .. }
            | Self::Renewed { .. }
            | Self::Cancel { .. }
            | Self::GoAway { .. } => Lane::Control,
        }
    }

    /// Unsolicited offers, cancellation and drain create no pending request.
    #[must_use]
    pub const fn request_id(&self) -> Option<NonZeroU64> {
        match self {
            Self::Welcome { request_id, .. }
            | Self::Heartbeat { request_id, .. }
            | Self::Pong { request_id }
            | Self::Renewed { request_id, .. }
            | Self::ReportAck { request_id, .. }
            | Self::Folded { request_id, .. } => Some(*request_id),
            Self::Offer { .. } | Self::Cancel { .. } | Self::GoAway { .. } => None,
        }
    }
}
