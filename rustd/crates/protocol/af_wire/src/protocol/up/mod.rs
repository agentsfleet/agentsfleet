//! Runner messages retain the existing body types and their validation owners.

use std::borrow::Cow;
use std::num::NonZeroU64;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{LEASES_MAX, StreamKind};
use crate::activity::ActivityRequest;
use crate::fold::FoldRequest;
use crate::lease::AcceptedKinds;
use crate::report::{RenewRequest, ReportRequest};
use crate::runner::HeartbeatRequest;

mod decode;
mod other;

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
#[derive(Debug, Clone, Serialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Up<
    'a,
    D: Clone = serde_json::Value,
    O: Clone = serde_json::Value,
    I: Clone = serde_json::Value,
> {
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
        /// Proposal whose final welcome may have been lost on the prior connection.
        #[serde(default, borrow, skip_serializing_if = "Option::is_none")]
        #[garde(inner(length(bytes, min = 36, max = 36)))]
        unconfirmed_connection_id: Option<Cow<'a, str>>,
    },
    /// Acknowledge the prospective owner before the server commits it.
    Confirm {
        /// The opening hello's correlation identifier.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// Exact prospective owner received in prepared.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        connection_id: Cow<'a, str>,
    },
    /// Replace, rather than increment, the advertised idle-worker count.
    Ready {
        /// Current idle workers, before subtracting offers still in flight.
        #[garde(range(max = LEASES_MAX))]
        credits: usize,
        /// Increasing availability update on this physical connection.
        #[garde(skip)]
        revision: NonZeroU64,
        /// Offers received on this connection; the daemon subtracts unseen offers.
        #[garde(skip)]
        observed_offers: u64,
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
    /// One physical authoring invocation; connection recovery must not redispatch it.
    AuthoringModel {
        #[garde(skip)]
        /// Session-local correlation identifier.
        request_id: NonZeroU64,
        /// Authoring lease whose fence and ownership scope this exchange.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        lease_id: Cow<'a, str>,
        #[garde(skip)]
        /// Current operation body; its domain service owns nested validation.
        request: Cow<'a, crate::authoring::ModelCall<D, O, I>>,
    },
    /// Acknowledged setup proposal or progress submission.
    AuthoringActivity {
        #[garde(skip)]
        /// Session-local correlation identifier.
        request_id: NonZeroU64,
        /// Authoring lease whose fence and ownership scope this exchange.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        lease_id: Cow<'a, str>,
        #[garde(skip)]
        /// Current operation body; its domain service owns nested validation.
        request: Cow<'a, crate::authoring::ActivityRequest<D, O, I>>,
    },
    /// Extend an existing private authoring lease without blocking model work.
    AuthoringRenew {
        #[garde(skip)]
        /// Session-local correlation identifier.
        request_id: NonZeroU64,
        /// Authoring lease whose fence and ownership scope this exchange.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        lease_id: Cow<'a, str>,
        #[garde(dive)]
        /// Current operation body; its domain service owns nested validation.
        request: crate::authoring::Renew,
    },
    /// A current authoring result retained in memory until durable acknowledgement.
    AuthoringReport {
        #[garde(skip)]
        /// Session-local correlation identifier.
        request_id: NonZeroU64,
        /// Authoring lease whose fence and ownership scope this exchange.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        lease_id: Cow<'a, str>,
        #[garde(dive)]
        /// Current operation body; its domain service owns nested validation.
        request: Cow<'a, crate::authoring::Report>,
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
        /// The current report retained in the runner worker's memory.
        #[serde(borrow)]
        #[garde(skip)]
        request: Box<Cow<'a, ReportRequest<'a>>>,
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

impl<D: Clone, O: Clone, I: Clone> Up<'_, D, O, I> {
    /// The only kind permitted to carry this message.
    #[must_use]
    pub const fn stream_kind(&self) -> StreamKind {
        match self {
            Self::Report { .. } | Self::AuthoringReport { .. } => StreamKind::Reports,
            Self::Activity { .. } => StreamKind::Activity,
            Self::Folds { .. } | Self::AuthoringModel { .. } | Self::AuthoringActivity { .. } => {
                StreamKind::Work
            }
            Self::Hello { .. }
            | Self::Confirm { .. }
            | Self::Ready { .. }
            | Self::Heartbeat { .. }
            | Self::Ping { .. }
            | Self::RenewBatch { .. }
            | Self::AuthoringRenew { .. } => StreamKind::Control,
        }
    }

    /// Ready and activity deliberately create no pending answer.
    #[must_use]
    pub const fn request_id(&self) -> Option<NonZeroU64> {
        match self {
            Self::Hello { request_id, .. }
            | Self::Confirm { request_id, .. }
            | Self::Heartbeat { request_id, .. }
            | Self::Ping { request_id }
            | Self::RenewBatch { request_id, .. }
            | Self::Report { request_id, .. }
            | Self::AuthoringModel { request_id, .. }
            | Self::AuthoringActivity { request_id, .. }
            | Self::AuthoringRenew { request_id, .. }
            | Self::AuthoringReport { request_id, .. }
            | Self::Folds { request_id, .. } => Some(*request_id),
            Self::Ready { .. } | Self::Activity { .. } => None,
        }
    }
}
