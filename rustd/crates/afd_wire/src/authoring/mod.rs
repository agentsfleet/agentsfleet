//! The private authoring protocol confers no fleet or tool authority.

mod activity;
mod lease;
mod meter;
mod model;
mod renew;
mod report;

pub use activity::{ActivityAccepted, ActivityEvent, ActivityRequest, CandidateVersion, Proposal};
pub use lease::{ContextMessage, ContextRole, Lease, Limits, ModelAccess, Pricing, SelectedModel};
pub use meter::{Knowledge, Meter, Usage};
pub use model::{ModelCall, ModelMessage, ModelRequest, ModelResult, ModelUsage, ProposalCall};
pub use renew::{Renew, Renewed};
pub use report::{Accounting, FailureReason, Outcome, Report, Reported};

/// A terminal, proposal or model-call envelope has this UTF-8 byte ceiling.
pub const ENVELOPE_BYTES_MAX: usize = 128 * 1024;
/// Progress is a bounded ephemeral hint, independent of durable answer text.
pub const PROGRESS_BYTES_MAX: usize = 4 * 1024;
/// Decimal counters fit the positive signed database integer range.
pub const DECIMAL_BYTES_MAX: usize = 19;
