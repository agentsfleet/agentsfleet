//! Fleet Bundle ingestion.
//!
//! Validation is a pure transformation: callers cannot obtain a storage key
//! until all untrusted bytes have passed the bundle limits and frontmatter
//! checks. Storage and source transport are separate boundary modules.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

#[cfg(test)]
use {bytes as _, object_store as _, tokio as _};

mod error;
mod frontmatter;
mod model;
mod prepare;
mod validate;

pub use error::{Error, InvalidBundle, Result};
pub use model::{ImportBody, PreparedBundle, Requirements, SourceKind, SupportFile, SupportManifest};
pub use prepare::prepare;
