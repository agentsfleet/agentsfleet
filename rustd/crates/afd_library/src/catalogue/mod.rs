//! Metadata-only platform Fleet-library administration.

mod etag;
mod model;
mod store;

pub use model::{DeleteLibrary, LibraryItem, LibraryPatch, LibraryRequirements, PatchLibrary};
pub use store::Libraries;
