//! Runner messages retain the existing body types and their validation owners.

use std::borrow::Cow;
use std::num::NonZeroU64;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{RUNS_MAX, StreamKind};
use crate::activity::ActivityRequest;
use crate::context::{ContextLoadRequest, ContextSaveRequest};
use crate::fold::FoldRequest;
use crate::report::{RenewRequest, ReportRequest};
use crate::run::AcceptedKinds;
use crate::runner::HeartbeatRequest;

mod decode;
mod other;

/// One run's usage records in the host's renewal batch.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Renewal<'a> {
    /// Canonical identifier, resolved by the run service after validation.
    #[serde(borrow)]
    #[garde(length(bytes, min = 36, max = 36))]
    pub run_id: Cow<'a, str>,
    /// Existing per-provider-request usage and its declared bounds.
    #[garde(dive)]
    pub request: RenewRequest,
}

/// Every message accepted from a runner. Requests use positive correlation ids.
#[derive(Debug, Clone, Serialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Up<'a> {
    /// Reconcile the running runs before accepting assignments.
    Hello {
        /// Session-local correlation identifier.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// Work domains this runner can execute.
        #[garde(skip)]
        accepted_kinds: AcceptedKinds,
        /// Runs the runner still considers active.
        #[serde(borrow)]
        #[garde(length(max = RUNS_MAX), inner(length(bytes, min = 36, max = 36)))]
        running: Vec<Cow<'a, str>>,
        /// Idle workers available to accept assignments.
        #[garde(range(max = RUNS_MAX))]
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
        /// Current idle workers, before subtracting assignments still in flight.
        #[garde(range(max = RUNS_MAX))]
        credits: usize,
        /// Increasing availability update on this physical connection.
        #[garde(skip)]
        revision: NonZeroU64,
        /// Assignments received on this connection; the daemon subtracts unseen assignments.
        #[garde(skip)]
        observed_assignments: u64,
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
        /// Independently fenced and acknowledged run requests.
        #[serde(borrow)]
        #[garde(length(min = 1, max = RUNS_MAX), dive)]
        runs: Vec<Renewal<'a>>,
    },
    /// Frame-specific validation remains in the existing activity service.
    Progress {
        /// Run whose target and fence scope the progress.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        run_id: Cow<'a, str>,
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
    /// Assign pending inputs at a model-call boundary under the run fence.
    Folds {
        /// Echoed by the folded-input answer.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// The live run requesting its next inputs.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        run_id: Cow<'a, str>,
        /// The fence and last consumed input cursor.
        #[garde(dive)]
        request: FoldRequest,
    },
    /// Event-owned recovery on the current ordinary run.
    TurnCheckpoint {
        /// Correlates this transmission, independently of semantic operation identity.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// Current run; the daemon derives every storage scope from it.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        run_id: Cow<'a, str>,
        /// Bounded command retained unchanged across transmission retries.
        #[garde(dive)]
        request: Cow<'a, crate::run::TurnCheckpointCommand>,
    },
    /// A setup run reads its draft's compacted context, under its fence.
    ContextLoad {
        /// Echoed by the loaded-context answer.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// The setup run; the daemon derives the draft from it.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        run_id: Cow<'a, str>,
        /// The fence the run holds.
        #[garde(dive)]
        request: ContextLoadRequest,
    },
    /// A setup run saves its draft's compacted context, under its fence.
    ContextSave {
        /// Echoed by the saved-context answer.
        #[garde(skip)]
        request_id: NonZeroU64,
        /// The setup run; the daemon derives the draft from it.
        #[serde(borrow)]
        #[garde(length(bytes, min = 36, max = 36))]
        run_id: Cow<'a, str>,
        /// The fence, the version it expects, and the checkpoint.
        #[garde(dive)]
        request: Cow<'a, ContextSaveRequest>,
    },
}

impl Up<'_> {
    /// The only kind permitted to carry this message.
    #[must_use]
    pub const fn stream_kind(&self) -> StreamKind {
        match self {
            Self::Report { .. } | Self::TurnCheckpoint { .. } => StreamKind::Reports,
            Self::Progress { .. } => StreamKind::Progress,
            Self::Folds { .. } | Self::ContextLoad { .. } | Self::ContextSave { .. } => {
                StreamKind::Work
            }
            Self::Hello { .. }
            | Self::Confirm { .. }
            | Self::Ready { .. }
            | Self::Heartbeat { .. }
            | Self::Ping { .. }
            | Self::RenewBatch { .. } => StreamKind::Control,
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
            | Self::TurnCheckpoint { request_id, .. }
            | Self::ContextLoad { request_id, .. }
            | Self::ContextSave { request_id, .. }
            | Self::Folds { request_id, .. } => Some(*request_id),
            Self::Ready { .. } | Self::Progress { .. } => None,
        }
    }
}
