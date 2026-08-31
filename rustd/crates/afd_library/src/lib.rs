//! Fleet Bundle ingestion.
//!
//! Validation is a pure transformation: callers cannot obtain a storage key
//! until all untrusted bytes have passed the bundle limits and frontmatter
//! checks. Storage and source transport are separate boundary modules.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

#[cfg(test)]
use {bytes as _, object_store as _, serde_json as _, tokio as _};

mod catalogue;
mod error;
mod frontmatter;
mod github;
mod model;
mod persist;
mod prepare;
mod preview;
mod snapshot;
mod source;
mod validate;

pub use catalogue::{
    DeleteLibrary, Destination, GalleryPage, Libraries, LibraryImports, LibraryItem, LibraryPatch,
    LibraryRequirements, PatchLibrary, Position, PublicLibraryItem, SummaryEntry, Tier,
};
pub use error::{Error, InvalidBundle, Result};
pub use github::{GithubSource, Repository, valid_revision};
pub use model::{
    ImportBody, PreparedBundle, Requirements, SourceKind, SupportFile, SupportManifest,
};
pub use persist::{BundleCatalog, ImportService, Onboarded};
pub use prepare::prepare;
pub use preview::{Preview, Previewer};
pub use snapshot::canonical_snapshot;
pub use source::{BundleSource, SourceFailure, SourceImporter};
