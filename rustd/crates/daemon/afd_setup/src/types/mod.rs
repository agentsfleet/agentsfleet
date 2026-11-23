//! Domain inputs and private setup projections shared by both chat clients.

mod creation;
mod failure;
mod input;
mod models;
mod output;
mod review;
mod revision;
mod source;

pub(crate) use creation::InitialStart;
pub use creation::{SaveFleet, SaveOrigin, SaveSource, Start, StartOutcome, StartState};
pub use failure::Failure;
pub use models::{ModelEligibility, ModelPricing, ModelPurpose, ModelsQuery, WorkspaceModel};
pub use review::{
    Action, ActionKind, Check, CheckKind, CheckState, Effective, Requirements, Review,
};
pub use revision::{AppliedRevision, ApplyRevision};
pub(crate) use source::{PinColumns, SourcePin};

pub use input::{CreateDraft, DraftPatch, ListQuery, MessageQuery, SendMessage, Source};
pub use output::{
    Accepted, Candidate, Created, Draft, DraftCard, DraftState, Message, MessageState, Page,
};
pub(crate) use output::{CANCELLED, EDITING, QUEUED, RUNNING};

pub(crate) const MAX_CURSOR_BYTES: usize = 4096;

pub(crate) fn present<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
