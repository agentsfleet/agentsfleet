//! The private authoring protocol confers no fleet or tool authority.

mod activity;
mod meter;
mod model;
mod proposal;
mod renew;
mod report;
mod run;

pub use activity::{ActivityAccepted, ActivityEvent, ActivityRequest, CandidateVersion, Proposal};
pub use meter::{Knowledge, Meter, Usage};
pub use model::{ModelCall, ModelMessage, ModelRequest, ModelResult, ModelUsage};
pub use proposal::{ProposalCall, ProposalRejection, REFUSAL_TEXT_BYTES_MAX, RefusalDetail};
pub use renew::{Renew, Renewed};
pub use report::{Accounting, FailureReason, Outcome, Report, Reported};
pub use run::{ContextMessage, ContextRole, Limits, ModelAccess, Pricing, Run, SelectedModel};

/// A terminal, proposal or model-call envelope has this UTF-8 byte ceiling.
pub const ENVELOPE_BYTES_MAX: usize = 128 * 1024;
/// Progress is a bounded ephemeral hint, independent of durable answer text.
pub const PROGRESS_BYTES_MAX: usize = 4 * 1024;
/// Decimal counters fit the positive signed database integer range.
pub const DECIMAL_BYTES_MAX: usize = 19;

/// Compiled authoring envelope; deployments may admit fewer calls.
pub const MODEL_CALLS_MAX: u32 = 8;
/// The private authoring protocol supports attempts up to five minutes.
pub const WALL_SECONDS_MAX: u32 = 300;
/// A refused proposal may receive at most one correction under this protocol.
pub const PROPOSAL_CORRECTIONS_MAX: u32 = 1;
