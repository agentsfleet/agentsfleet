//! Runner messages retain the existing body types and their validation owners.

use std::borrow::Cow;
use std::num::NonZeroU64;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{LEASES_MAX, Lane};
use crate::activity::ActivityRequest;
use crate::fold::FoldRequest;
use crate::lease::AcceptedKinds;
use crate::report::{RenewRequest, ReportRequest};
use crate::runner::HeartbeatRequest;

/// One lease's usage records in the host's renewal batch.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Renewal<'a> {
    /// Canonical identifier, resolved by the lease service after validation.
    #[serde(borrow)]
    #[garde(length(bytes, min = 36, max = 36))]
    pub lease_id: Cow<'a, str>,
    /// Existing per-provider-request usage and its declared bounds.
    #[garde(dive)]
    pub request: RenewRequest,
}

/// Every message accepted from a runner. Requests use positive correlation ids.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Up<'a> {
    /// Reconcile the running leases before accepting offers.
    Hello {
        /// Session-local correlation identifier.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// Work domains this runner can execute.
        #[garde(skip)]
        accepted_kinds: AcceptedKinds,
        /// Leases the runner still considers active.
        #[serde(borrow)]
        #[garde(length(max = LEASES_MAX), inner(length(bytes, min = 36, max = 36)))]
        running: Vec<Cow<'a, str>>,
        /// Idle workers available to accept offers.
        #[garde(range(max = LEASES_MAX))]
        credits: usize,
    },
    /// Replace, rather than increment, the advertised idle-worker count.
    Ready {
        /// Current idle workers, replacing the previous count.
        #[garde(range(max = LEASES_MAX))]
        credits: usize,
    },
    /// The heartbeat service retains capability and self-test validation.
    Heartbeat {
        /// Session-local correlation identifier.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// Existing capability, self-test and held-sandbox report.
        #[serde(borrow)]
        #[garde(skip)]
        request: HeartbeatRequest<'a>,
    },
    /// Application responsiveness, not merely transport acknowledgement.
    Ping {
        /// Echoed by pong to prove application responsiveness.
        #[garde(skip)]
        request_id: NonZeroU64,
    },
    /// Each entry preserves the existing usage request.
    RenewBatch {
        /// Echoed by the batch answer.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// Independently fenced and acknowledged lease requests.
        #[serde(borrow)]
        #[garde(length(min = 1, max = LEASES_MAX), dive)]
        leases: Vec<Renewal<'a>>,
    },
    /// Frame-specific validation remains in the existing activity service.
    Activity {
        /// Lease whose target and fence scope the progress.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        lease_id: Cow<'a, str>,
        /// The existing bounded activity body.
        #[serde(borrow)]
        #[garde(skip)]
        request: ActivityRequest<'a>,
    },
    /// The existing report parser owns its nested usage and tool bounds.
    Report {
        /// Echoed by the report acknowledgement.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// The exact report held in the durable spool.
        #[serde(borrow)]
        #[garde(skip)]
        request: Box<ReportRequest<'a>>,
    },
    /// Claim pending inputs at a model-call boundary under the lease fence.
    Folds {
        /// Echoed by the folded-input answer.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// The live lease requesting its next inputs.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        lease_id: Cow<'a, str>,
        /// The fence and last consumed input cursor.
        #[garde(dive)]
        request: FoldRequest,
    },
}

impl Up<'_> {
    /// The only lane permitted to carry this message.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Report { .. } => Lane::Reports,
            Self::Activity { .. } => Lane::Activity,
            Self::Folds { .. } => Lane::Work,
            Self::Hello { .. }
            | Self::Ready { .. }
            | Self::Heartbeat { .. }
            | Self::Ping { .. }
            | Self::RenewBatch { .. } => Lane::Control,
        }
    }

    /// Ready and activity deliberately create no pending answer.
    #[must_use]
    pub const fn request_id(&self) -> Option<NonZeroU64> {
        match self {
            Self::Hello { request_id, .. }
            | Self::Heartbeat { request_id, .. }
            | Self::Ping { request_id }
            | Self::RenewBatch { request_id, .. }
            | Self::Report { request_id, .. }
            | Self::Folds { request_id, .. } => Some(*request_id),
            Self::Ready { .. } | Self::Activity { .. } => None,
        }
    }
}
