//! Metadata-only platform Fleet-library administration.

mod etag;
mod gallery;
mod import;
mod model;
mod public;
mod store;

const VISIBILITY_DRAFT: &str = "draft";
const VISIBILITY_PUBLIC: &str = "public";

pub use gallery::{GalleryPage, Position, SummaryEntry, Tier};
pub use import::LibraryImports;
pub use model::{DeleteLibrary, LibraryItem, LibraryPatch, LibraryRequirements, PatchLibrary};
pub use public::PublicLibraryItem;
pub use store::Libraries;
