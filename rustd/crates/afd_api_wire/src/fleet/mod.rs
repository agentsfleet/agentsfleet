//! Shared fleets expose native setup and resumable progress; chat remains creator-private.

mod collection;
mod input;
mod memory;
mod output;
mod revision;

pub use collection::{FleetSummary, FleetsResponse, Triggers};
pub use input::{PatchFleetRequest, PatchStatus, SaveFleetRequest, SaveSource, Start};
pub use memory::{MemoryAccess, MemoryAccessRequest};
pub use output::{FleetResponse, WebhookUrl};
pub use revision::{AppliedRevision, ApplyRevision};
