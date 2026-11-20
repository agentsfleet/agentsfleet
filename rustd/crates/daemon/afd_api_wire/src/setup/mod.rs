//! Native workspace conversation shapes shared by browser and terminal clients.
//!
//! Generic setup/edit fields let the daemon deserialize through its shared
//! validator while this wire-only crate retains no datastore dependency.

mod context;
mod input;
mod models;
mod output;
#[cfg(feature = "openapi")]
mod schema;

pub use context::{Conversation, SetupActivity, SetupContext};

pub use models::{Eligibility, Pricing, WorkspaceModel};

pub use input::{CreateDraftRequest, DraftPatchRequest, MessageRequest, Source};
pub use output::{
    Accepted, Candidate, Created, CursorPage, Draft, DraftCard, Failure, Message, Stopped,
};

pub(crate) fn present<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
