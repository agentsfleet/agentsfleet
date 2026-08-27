//! Owned inputs and outputs for pure bundle preparation.

use serde::Serialize;

/// Where a bundle's untrusted bytes originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A first-party gallery template.
    Template,
    /// A direct caller upload.
    Upload,
    /// A public GitHub repository snapshot.
    Github,
}

/// One non-root file supplied by a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportFile {
    /// Relative, slash-separated path inside the bundle.
    pub path: String,
    /// Untrusted file bytes.
    pub content: Vec<u8>,
}

/// Complete caller input to pure bundle preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBody {
    /// Source category persisted as metadata.
    pub source_kind: SourceKind,
    /// Caller-readable source locator.
    pub source_ref: String,
    /// Git reference actually fetched, when applicable.
    pub source_revision: Option<String>,
    /// Required `SKILL.md` bytes.
    pub skill_markdown: Vec<u8>,
    /// Optional `TRIGGER.md` bytes.
    pub trigger_markdown: Option<Vec<u8>>,
    /// All remaining bundle files.
    pub support_files: Vec<SupportFile>,
}

/// Safe metadata persisted for one support file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupportManifest {
    /// Validated relative path.
    pub path: String,
    /// Exact byte count.
    pub size_bytes: usize,
    /// Lowercase SHA-256 digest of the content.
    pub sha256: String,
}

/// Names a workspace must satisfy before installing the bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Requirements {
    /// Credential names only, never values.
    pub credentials: Vec<String>,
    /// Tool identifiers requested by the trigger.
    pub tools: Vec<String>,
    /// Declared outbound-network hosts.
    pub network_hosts: Vec<String>,
    /// Validated support paths.
    pub support_files: Vec<String>,
    /// Whether this bundle supplied a trigger document.
    pub trigger_present: bool,
}

/// Validated, content-addressed metadata ready for a write boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBundle {
    /// Fleet identity from skill frontmatter.
    pub name: String,
    /// Human-readable skill description.
    pub description: String,
    /// Whole-bundle lowercase SHA-256 digest.
    pub content_hash: String,
    /// Immutable object-storage key derived from the digest.
    pub snapshot_key: String,
    /// Content-free support-file manifest.
    pub support_manifest: Vec<SupportManifest>,
    /// Parsed requirement names.
    pub requirements: Requirements,
}
