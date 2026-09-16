//! The failure vocabulary for bundle ingestion and external sources.
//!
//! One error type under the `afd_core::error_shell!` hull its sibling crates
//! carry: a struct over a private kind, with the captured backtrace, the
//! `[CODE]` rendering and the self-skipping `source()` generated rather than
//! written here again. The boxed kind also keeps `Result` pointer-sized on the
//! `Ok` path, which matters on a crate whose happy path streams archive bytes.
//!
//! # Two kinds carry data rather than a cause
//!
//! [`ErrorKind::Invalid`] and [`ErrorKind::Source`] hold an [`InvalidBundle`]
//! and a [`SourceFailure`], which are VERDICTS this crate reached rather than
//! failures underneath it — neither implements `Error`, so neither is a
//! `source()`. They compose by a hand-written `From` in `raise` instead of
//! through `error_lifts!`, which is the same composition by a different door.

use afd_core::error_code::{
    CATALOG_ID_COLLISION, ErrorCode, FLEET_BUNDLE_CREDENTIAL_NAME_INVALID,
    FLEET_BUNDLE_FETCH_FAILED, FLEET_BUNDLE_INVALID, FLEET_BUNDLE_STORAGE_UNAVAILABLE,
    INTERNAL_DB_QUERY, INTERNAL_DB_UNAVAILABLE, INTERNAL_OPERATION_FAILED, PAYLOAD_TOO_LARGE,
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
            Self::EmbeddedCredential => "bundle document contains credential material",
            Self::RequirementsTooLarge => "declared requirements exceed their bounds",
        })
    }
}

mod raise;

#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;
pub(crate) use self::raise::{catalog_id_collision, database, storage_unavailable};

afd_core::error_shell!(
    /// A Fleet Bundle failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every fallible operation owned by this crate.
///
/// Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
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
    /// Trigger frontmatter has valid YAML but violates the runtime schema.
    #[error("invalid Fleet Bundle: TRIGGER.md runtime configuration is invalid")]
    TriggerConfig { source: afd_fleet_runtime::Error },
    /// Immutable snapshot storage did not accept a write.
    #[error("Fleet Bundle snapshot storage failed")]
    Storage { source: object_store::Error },
    /// Snapshot storage is not configured for a bundle carrying support files.
    #[error("Fleet Bundle snapshot storage is unavailable")]
    StorageUnavailable,
    /// A different source already owns the bundle's frontmatter name.
    #[error("Fleet Bundle catalogue id is already owned by {incumbent}")]
    CatalogIdCollision {
        /// Existing source repository or upload identity.
        incumbent: String,
    },
    /// A pool connection could not be acquired.
    #[error(transparent)]
    Pool { source: afd_db::Error },
    /// Persisted catalogue JSON did not match its schema.
    #[error("Fleet Bundle catalogue contains malformed JSON")]
    CatalogueJson { source: serde_json::Error },
    /// Validated files could not be encoded as a canonical tar.
    #[error("Fleet Bundle snapshot encoding failed")]
    Snapshot { source: std::io::Error },
    /// A source returned an ordinary, typed failure class.
    #[error("Fleet Bundle source failed: {0}")]
    Source(SourceFailure),
    /// The GitHub transport failed before returning a classified response.
    #[error("Fleet Bundle GitHub request failed")]
    Github { source: reqwest::Error },
    /// A downloaded source archive could not be decoded completely.
    #[error("Fleet Bundle archive is corrupt or truncated")]
    Archive { source: std::io::Error },
    /// The runtime could not complete archive extraction on its blocking pool.
    #[error("Fleet Bundle archive extraction task failed")]
    ArchiveTask { source: tokio::task::JoinError },
    /// A GitHub redirect is not a valid URL.
    #[error("Fleet Bundle source returned an invalid redirect")]
    Redirect { source: url::ParseError },
    /// A tar entry path is not UTF-8.
    #[error("Fleet Bundle archive contains a non-UTF-8 path")]
    ArchivePath { source: std::str::Utf8Error },
    /// The host could not draw the entropy an onboarded entry is minted from.
    ///
    /// Only the workspace tier mints: the platform catalogue is keyed by the
    /// bundle's own name, so it draws nothing.
    #[error("could not draw the entropy a Fleet Bundle entry is minted from")]
    Entropy {
        /// The entropy source's refusal.
        #[source]
        source: afd_crypto::error::Error,
    },
    /// A minted entry identifier was not well-formed.
    #[error("a minted Fleet Bundle entry identifier was not well-formed")]
    Mint {
        /// The identifier's own refusal.
        #[source]
        source: afd_core::error::Error,
    },
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
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Invalid(
                InvalidBundle::SkillTooLarge
                | InvalidBundle::TriggerTooLarge
                | InvalidBundle::TooManySupportFiles
                | InvalidBundle::SupportFileTooLarge
                | InvalidBundle::SupportFilesTooLarge
                | InvalidBundle::RequirementsTooLarge,
            )
            | ErrorKind::Source(SourceFailure::ArchiveTooLarge | SourceFailure::TooManyFiles) => {
                PAYLOAD_TOO_LARGE
            }
            // Ahead of the catch-all below: a name the vault will not store is
            // fixed by renaming it, not by re-packaging the bundle.
            ErrorKind::TriggerConfig { source: refusal }
                if matches!(
                    refusal.class(),
                    afd_fleet_runtime::Class::InvalidCredentialRef
                ) =>
            {
                FLEET_BUNDLE_CREDENTIAL_NAME_INVALID
            }
            ErrorKind::Invalid(_)
            | ErrorKind::FrontmatterUtf8 { .. }
            | ErrorKind::FrontmatterYaml { .. }
            | ErrorKind::TriggerConfig { .. } => FLEET_BUNDLE_INVALID,
            ErrorKind::Storage { .. }
            | ErrorKind::StorageUnavailable
            | ErrorKind::Snapshot { .. } => FLEET_BUNDLE_STORAGE_UNAVAILABLE,
            ErrorKind::CatalogIdCollision { .. } => CATALOG_ID_COLLISION,
            ErrorKind::Pool { .. } => INTERNAL_DB_UNAVAILABLE,
            // Neither is the caller's to correct: a host that cannot draw
            // entropy and a mint that produced something `Uuid7` refuses are
            // both this instance's failure, and both answer the same internal
            // code the credential plane gives them.
            ErrorKind::Entropy { .. } | ErrorKind::Mint { .. } => INTERNAL_OPERATION_FAILED,
            ErrorKind::CatalogueJson { .. } | ErrorKind::Database { .. } => INTERNAL_DB_QUERY,
            ErrorKind::Source(SourceFailure::InvalidReference | SourceFailure::UnsafeArchive) => {
                FLEET_BUNDLE_INVALID
            }
            ErrorKind::Source(_)
            | ErrorKind::Github { .. }
            | ErrorKind::Archive { .. }
            | ErrorKind::ArchiveTask { .. }
            | ErrorKind::Redirect { .. }
            | ErrorKind::ArchivePath { .. } => FLEET_BUNDLE_FETCH_FAILED,
        }
    }

    /// Whether retrying without changing the request is safe.
    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Storage { .. }
                | ErrorKind::StorageUnavailable
                | ErrorKind::Pool { .. }
                | ErrorKind::Source(SourceFailure::RateLimited)
                | ErrorKind::Github { .. }
                | ErrorKind::Database { .. }
        )
    }

    /// Client-safe detail exposed to the HTTP shell.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Pool { .. } => "Database unavailable",
            ErrorKind::CatalogueJson { .. } | ErrorKind::Database { .. } => "Database error",
            // Nothing a caller can act on, and nothing about the bundle they
            // sent: this instance could not mint an identifier for the row it
            // was about to write.
            ErrorKind::Entropy { .. } | ErrorKind::Mint { .. } => {
                "Onboarding could not be completed"
            }
            ErrorKind::Invalid(
                InvalidBundle::SkillTooLarge
                | InvalidBundle::TriggerTooLarge
                | InvalidBundle::TooManySupportFiles
                | InvalidBundle::SupportFileTooLarge
                | InvalidBundle::SupportFilesTooLarge
                | InvalidBundle::RequirementsTooLarge,
            )
            | ErrorKind::Source(SourceFailure::ArchiveTooLarge | SourceFailure::TooManyFiles) => {
                "Fleet Bundle exceeds a configured size cap"
            }
            ErrorKind::Invalid(_)
            | ErrorKind::FrontmatterUtf8 { .. }
            | ErrorKind::FrontmatterYaml { .. }
            | ErrorKind::TriggerConfig { .. }
            | ErrorKind::Source(SourceFailure::InvalidReference | SourceFailure::UnsafeArchive) => {
                "Fleet Bundle is invalid"
            }
            ErrorKind::Source(_)
            | ErrorKind::Github { .. }
            | ErrorKind::Archive { .. }
            | ErrorKind::ArchiveTask { .. }
            | ErrorKind::Redirect { .. }
            | ErrorKind::ArchivePath { .. } => "Fleet Bundle fetch failed",
            ErrorKind::Storage { .. }
            | ErrorKind::StorageUnavailable
            | ErrorKind::Snapshot { .. } => "Fleet Bundle storage unavailable",
            ErrorKind::CatalogIdCollision { .. } => "Fleet Bundle catalogue id is already in use",
        }
    }

    /// Whether the backing database could not be reached at all.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Pool { .. })
    }

    /// Existing source when this error is an id collision.
    #[must_use]
    pub fn collision_incumbent(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::CatalogIdCollision { incumbent } => Some(incumbent),
            _ => None,
        }
    }
}

/// The result returned by fallible operations in this crate.
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
