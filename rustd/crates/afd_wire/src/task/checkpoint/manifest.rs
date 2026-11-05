//! Complete logical file mappings; private content stays in encrypted objects.

use crate::task::DecisionTaskVersion;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// One bounded file represented by immutable encrypted chunks in order.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointFile {
    /// Declared workspace-relative path; never followed during restoration.
    #[garde(length(bytes, min = 1, max = 4096), custom(relative_path))]
    pub path: String,
    /// Ordinary file executable bit.
    #[garde(skip)]
    pub executable: bool,
    /// Logical length; empty files require no objects.
    #[garde(range(max = 25_769_803_776))]
    pub size: u64,
    /// Native chunk identities, in byte order.
    #[garde(
        length(max = 3072),
        inner(
            length(bytes, min = 36, max = 36),
            pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        )
    )]
    pub objects: Vec<String>,
}

/// Complete logical entry or explicit deletion relative to the retained base.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckpointEntry {
    /// Ordinary file; special files are never captured.
    File(#[garde(dive)] CheckpointFile),
    /// An explicit directory, including empty directories.
    Directory {
        /// Workspace-relative directory path, including an otherwise empty directory.
        #[garde(length(bytes, min = 1, max = 4096), custom(relative_path))]
        path: String,
    },
    /// A relative non-escaping symlink, validated against its parent natively.
    Symlink {
        /// Workspace-relative location at which the link is restored.
        #[garde(length(bytes, min = 1, max = 4096), custom(relative_path))]
        path: String,
        /// Link destination relative to its parent, checked for workspace escape.
        #[garde(length(bytes, min = 1, max = 4096), custom(relative_link))]
        target: String,
    },
    /// Explicit removal from the verified base.
    Remove {
        /// Workspace-relative base entry to remove, including any descendants.
        #[garde(length(bytes, min = 1, max = 4096), custom(relative_path))]
        path: String,
    },
}

impl CheckpointEntry {
    /// Common path for globally ordered native validation.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::File(file) => &file.path,
            Self::Directory { path } | Self::Symlink { path, .. } | Self::Remove { path } => path,
        }
    }
}

/// At most one mebibyte of the complete logical manifest.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPage {
    /// Zero-based immutable page position.
    #[garde(range(max = 63))]
    pub page_number: u32,
    /// Logical entries, bounded again by complete serialized bytes.
    #[garde(length(max = 4096), dive)]
    pub entries: Vec<CheckpointEntry>,
    /// Compatible private conversation objects; not independently selected history.
    #[garde(
        length(max = 256),
        inner(
            length(bytes, min = 36, max = 36),
            pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        )
    )]
    pub history_objects: Vec<String>,
}

/// Publish one immutable manifest page with its exact digest.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPageInput {
    /// Existing lease fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)]
    pub task: DecisionTaskVersion,
    /// Live writer pin.
    #[garde(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    )]
    pub pin_id: String,
    /// Canonical SHA-256 of this complete page.
    #[garde(custom(super::digest))]
    pub digest: String,
    /// Bounded typed representation.
    #[garde(dive, custom(page_bound))]
    pub contents: CheckpointPage,
}

fn page_bound(page: &CheckpointPage, _context: &()) -> garde::Result {
    if afd_validate::serialized_within(page, super::CHECKPOINT_PAGE_BYTES) {
        Ok(())
    } else {
        Err(garde::Error::new("manifest page exceeds one mebibyte"))
    }
}

fn relative_path(value: &str, _context: &()) -> garde::Result {
    if value.split('/').any(|part| matches!(part, "" | "." | ".."))
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        Err(garde::Error::new("expected a safe relative path"))
    } else {
        Ok(())
    }
}

fn relative_link(value: &str, _context: &()) -> garde::Result {
    if value.starts_with('/') || value.contains('\\') || value.chars().any(char::is_control) {
        Err(garde::Error::new("expected a relative symlink"))
    } else {
        Ok(())
    }
}
