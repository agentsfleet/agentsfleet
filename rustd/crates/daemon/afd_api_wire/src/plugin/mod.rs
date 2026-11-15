//! Explicit native Plugin publication and immutable reusable history.

mod input;
mod output;

pub use input::{Publish, PublishRevision};
pub use output::{Card, Plugin, Published, Revision, RevisionRead, RevisionSummary};
