//! The failure vocabulary for bundle ingestion and external sources.

use afd_core::error_code::{ErrorCode, FLEET_BUNDLE_INVALID, FLEET_BUNDLE_STORAGE_UNAVAILABLE};

/// The precise validation rule an untrusted bundle violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidBundle {
    /// The declared source is not supported.
    SourceKind,
    /// The source reference exceeds its wire bound.
    SourceRefTooLong,
    /// No skill document was supplied.
    MissingSkill,
    /// The skill document exceeds its byte bound.
    SkillTooLarge,
    /// Skill frontmatter is malformed or semantically invalid.
    InvalidSkill,
    /// The trigger document is empty or exceeds its byte bound.
    TriggerTooLarge,
    /// Trigger frontmatter is malformed.
    InvalidTrigger,
    /// Skill and trigger identities differ.
    NameMismatch,
    /// More support files were supplied than one bundle admits.
    TooManySupportFiles,
    /// A support path could escape or collide with a root document.
    UnsafeSupportPath,
    /// One support file exceeds its byte bound.
    SupportFileTooLarge,
    /// Aggregate support bytes exceed their bound.
    SupportFilesTooLarge,
    /// Support bytes contain a known credential-value shape.
    EmbeddedCredential,
    /// Requirement counts or individual names exceed their bounds.
    RequirementsTooLarge,
}

impl core::fmt::Display for InvalidBundle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::SourceKind => "source kind is not supported",
            Self::SourceRefTooLong => "source reference exceeds 512 bytes",
            Self::MissingSkill => "SKILL.md is required",
            Self::SkillTooLarge => "SKILL.md exceeds 200 KiB",
            Self::InvalidSkill => "SKILL.md frontmatter is invalid",
            Self::TriggerTooLarge => "TRIGGER.md is empty or exceeds 200 KiB",
            Self::InvalidTrigger => "TRIGGER.md frontmatter is invalid",
            Self::NameMismatch => "SKILL.md and TRIGGER.md names differ",
            Self::TooManySupportFiles => "bundle has more than 32 support files",
            Self::UnsafeSupportPath => "support-file path is unsafe",
            Self::SupportFileTooLarge => "a support file exceeds 64 KiB",
            Self::SupportFilesTooLarge => "support files exceed 256 KiB in total",
            Self::EmbeddedCredential => "support file contains credential material",
            Self::RequirementsTooLarge => "declared requirements exceed their bounds",
        })
    }
}

/// Every fallible operation owned by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Untrusted bundle bytes were refused before any write.
    #[error("invalid Fleet Bundle: {0}")]
    Invalid(InvalidBundle),
    /// Immutable snapshot storage did not accept a write.
    #[error("Fleet Bundle snapshot storage failed")]
    Storage(#[source] object_store::Error),
}

impl Error {
    /// The stable product error code exposed at the HTTP boundary.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Invalid(_) => FLEET_BUNDLE_INVALID,
            Self::Storage(_) => FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        }
    }

    /// Whether retrying without changing the request is safe.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(self, Self::Storage(_))
    }
}

impl From<InvalidBundle> for Error {
    fn from(value: InvalidBundle) -> Self {
        Self::Invalid(value)
    }
}

/// The result returned by fallible operations in this crate.
pub type Result<T, E = Error> = core::result::Result<T, E>;
