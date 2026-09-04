//! Owned inputs and outputs for pure bundle preparation.

use garde::Validate;
use serde::Serialize;

use crate::validate::{
    MAX_MARKDOWN_LEN, MAX_SOURCE_REF_LEN, MAX_SUPPORT_FILE_LEN, MAX_SUPPORT_FILES,
};

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

/// The stored and accepted spelling of a template source.
const KIND_TEMPLATE: &str = "template";

/// The stored and accepted spelling of an upload source.
const KIND_UPLOAD: &str = "upload";

/// The stored and accepted spelling of a GitHub source.
const KIND_GITHUB: &str = "github";

impl SourceKind {
    /// The stored spelling — the bytes in `source_kind`, and the ones a caller
    /// names on the wire.
    ///
    /// One declaration for both directions, so a kind this daemon WRITES is one
    /// it would accept back (RULE UFS). A request naming a spelling the store
    /// never emits would be a round trip that loses the row's own identity.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Template => KIND_TEMPLATE,
            Self::Upload => KIND_UPLOAD,
            Self::Github => KIND_GITHUB,
        }
    }

    /// The kind a spelling names, or nothing for one this daemon does not serve.
    ///
    /// `None` rather than a default: a source kind silently becoming `Upload`
    /// would skip the fetch a caller asked for and store an empty bundle.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Template, Self::Upload, Self::Github]
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

/// One non-root file supplied by a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Validate)]
pub struct SupportFile {
    /// Relative, slash-separated path inside the bundle.
    ///
    /// Path safety is not a bound, so it is a custom rule: the refusal turns on
    /// traversal components and root-document collision, neither of which a
    /// length expresses.
    #[garde(custom(crate::validate::safe_path))]
    pub path: String,
    /// Untrusted file bytes.
    #[garde(
        length(max = MAX_SUPPORT_FILE_LEN),
        custom(crate::validate::no_credential_bytes)
    )]
    pub content: Vec<u8>,
}

/// Complete caller input to pure bundle preparation.
///
/// The bounds below are the whole of this type's size contract. Two of them
/// carry a `custom` rule beside the `length` rule on the same field, because
/// the two ends of one range answer DIFFERENT public codes: an empty
/// `SKILL.md` is a malformed bundle (400) while an oversized one is a payload
/// that exceeded its cap (413), and a report that could not tell them apart
/// could not answer either. `crate::validate` maps each rule back to the
/// [`crate::InvalidBundle`] variant that classification rides on.
#[derive(Debug, Clone, PartialEq, Eq, Validate)]
#[garde(custom(crate::validate::aggregate))]
pub struct ImportBody {
    /// Source category persisted as metadata.
    #[garde(skip)]
    pub source_kind: SourceKind,
    /// Caller-readable source locator.
    ///
    /// Bounded in bytes, not characters: the cap exists to keep an untrusted
    /// locator off the wire and out of a column, and both count bytes.
    #[garde(length(bytes, max = MAX_SOURCE_REF_LEN))]
    pub source_ref: String,
    /// Git reference actually fetched, when applicable.
    #[garde(skip)]
    pub source_revision: Option<String>,
    /// Required `SKILL.md` bytes.
    #[garde(length(max = MAX_MARKDOWN_LEN), custom(crate::validate::skill_present))]
    pub skill_markdown: Vec<u8>,
    /// Optional `TRIGGER.md` bytes.
    ///
    /// Absent is legal; present-and-empty is not. `inner` reaches through the
    /// `Option` so the absent case is skipped rather than refused.
    #[garde(
        inner(length(max = MAX_MARKDOWN_LEN)),
        custom(crate::validate::trigger_non_empty)
    )]
    pub trigger_markdown: Option<Vec<u8>>,
    /// All remaining bundle files.
    ///
    /// `dive` runs each file's own rules and reports the INDEX that broke
    /// them — `support_files[3].content` — where a hand-rolled loop refuses
    /// "a support file" and leaves the caller to find which one.
    #[garde(length(max = MAX_SUPPORT_FILES), dive)]
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
