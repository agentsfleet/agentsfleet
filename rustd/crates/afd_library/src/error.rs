//! The failure vocabulary for bundle ingestion and external sources.

use afd_core::error_code::{
    ErrorCode, FLEET_BUNDLE_FETCH_FAILED, FLEET_BUNDLE_INVALID, FLEET_BUNDLE_STORAGE_UNAVAILABLE,
};

use crate::source::SourceFailure;

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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
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
    /// A root document is not UTF-8; the decoder remains the source.
    #[error("invalid Fleet Bundle: {document} is not UTF-8")]
    FrontmatterUtf8 {
        /// Root document being parsed.
        document: &'static str,
        /// Decoder refusal.
        #[source]
        source: std::str::Utf8Error,
    },
    /// YAML frontmatter is malformed; the parser remains the source.
    #[error("invalid Fleet Bundle: {document} frontmatter is malformed")]
    FrontmatterYaml {
        /// Root document being parsed.
        document: &'static str,
        /// YAML parser refusal.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// Immutable snapshot storage did not accept a write.
    #[error("Fleet Bundle snapshot storage failed")]
    Storage(#[source] object_store::Error),
    /// Validated metadata could not be committed to the catalogue.
    #[error("Fleet Bundle catalogue write failed")]
    Catalogue(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A pool connection could not be acquired.
    #[error(transparent)]
    Pool(#[from] afd_db::Error),
    /// Persisted catalogue JSON did not match its schema.
    #[error("Fleet Bundle catalogue contains malformed JSON")]
    CatalogueJson(#[from] serde_json::Error),
    /// Validated files could not be encoded as a canonical tar.
    #[error("Fleet Bundle snapshot encoding failed")]
    Snapshot(#[source] std::io::Error),
    /// A source returned an ordinary, typed failure class.
    #[error("Fleet Bundle source failed: {0}")]
    Source(SourceFailure),
    /// The GitHub transport failed before returning a classified response.
    #[error("Fleet Bundle GitHub request failed")]
    Github(#[source] reqwest::Error),
    /// A downloaded source archive could not be decoded completely.
    #[error("Fleet Bundle archive is corrupt or truncated")]
    Archive(#[source] std::io::Error),
    /// A GitHub redirect is not a valid URL.
    #[error("Fleet Bundle source returned an invalid redirect")]
    Redirect(#[source] url::ParseError),
    /// A tar entry path is not UTF-8.
    #[error("Fleet Bundle archive contains a non-UTF-8 path")]
    ArchivePath(#[source] std::str::Utf8Error),
    /// A catalogue query failed with its statement context retained.
    #[error("Fleet Bundle catalogue query failed during {context}")]
    Database {
        /// Operation being attempted.
        context: &'static str,
        /// Database refusal.
        #[source]
        source: sqlx::Error,
    },
}

impl Error {
    /// The stable product error code exposed at the HTTP boundary.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Invalid(_) | Self::FrontmatterUtf8 { .. } | Self::FrontmatterYaml { .. } => {
                FLEET_BUNDLE_INVALID
            }
            Self::Storage(_)
            | Self::Catalogue(_)
            | Self::Pool(_)
            | Self::CatalogueJson(_)
            | Self::Snapshot(_)
            | Self::Database { .. } => FLEET_BUNDLE_STORAGE_UNAVAILABLE,
            Self::Source(_)
            | Self::Github(_)
            | Self::Archive(_)
            | Self::Redirect(_)
            | Self::ArchivePath(_) => FLEET_BUNDLE_FETCH_FAILED,
        }
    }

    /// Whether retrying without changing the request is safe.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Storage(_)
                | Self::Catalogue(_)
                | Self::Pool(_)
                | Self::Source(SourceFailure::RateLimited)
                | Self::Github(_)
                | Self::Database { .. }
        )
    }
}

impl From<InvalidBundle> for Error {
    fn from(value: InvalidBundle) -> Self {
        Self::Invalid(value)
    }
}

impl From<object_store::Error> for Error {
    fn from(value: object_store::Error) -> Self {
        Self::Storage(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Snapshot(value)
    }
}

impl From<SourceFailure> for Error {
    fn from(value: SourceFailure) -> Self {
        Self::Source(value)
    }
}

impl Error {
    pub(crate) fn catalogue(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Catalogue(Box::new(source))
    }

    pub(crate) fn database(context: &'static str) -> impl Fn(sqlx::Error) -> Self {
        move |source| Self::Database { context, source }
    }
}

/// The result returned by fallible operations in this crate.
pub type Result<T, E = Error> = core::result::Result<T, E>;
