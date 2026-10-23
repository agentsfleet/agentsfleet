//! Domain inputs and private setup projections shared by both chat clients.

mod creation;
mod failure;
mod input;
mod library;
mod models;
mod output;
mod platform;
mod review;
mod revision;
mod source;

pub(crate) use creation::InitialStart;
pub use creation::{SaveFleet, SaveSource, Start, StartOutcome, StartState};
pub use failure::{Failure, FailureCause};
pub use models::{ModelEligibility, ModelPricing, ModelPurpose, ModelsQuery, WorkspaceModel};
pub use platform::{PlatformLibrary, PlatformPatch};
pub use review::{
    Action, ActionKind, Check, CheckKind, CheckState, Effective, Requirements, Review,
};
pub use revision::{AppliedRevision, ApplyRevision};
pub(crate) use source::{PinColumns, SourcePin};

pub use input::{CreateDraft, DraftPatch, ListQuery, MessageQuery, SendMessage, Source};
pub use library::{
    Library, LibraryCard, LibraryQuery, LibraryRevision, Publication, PublishLibrary,
    PublishRevision, Published, RevisionSummary,
};
pub use output::{
    Accepted, Candidate, Created, Draft, DraftCard, DraftState, Message, MessageState, Page,
};
pub(crate) use output::{CANCELLED, EDITING, QUEUED, RUNNING};

/// Upper bound on a chat message's UTF-8 bytes.
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024;
/// Bound on opaque model references, independent of their current spelling.
pub const MAX_MODEL_REF_BYTES: usize = 256;
/// Bound on a human draft title, separate from a setup slug.
pub const MAX_TITLE_CHARS: usize = 100;
pub(crate) const MAX_CURSOR_BYTES: usize = 4096;
pub(crate) const MAX_PAGE_SIZE: u32 = 100;
/// Longest chat search a list accepts, in characters.
pub(crate) const MAX_QUERY_CHARS: usize = 200;
pub(crate) const DEFAULT_PAGE_SIZE: u32 = 50;

pub(crate) fn present<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
