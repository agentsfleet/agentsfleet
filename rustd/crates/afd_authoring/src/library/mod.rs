//! Explicit publication copies native candidates while keeping drafts private.

mod history;
mod list;
mod platform;
mod publish;
mod read;
mod remove;
mod source;
mod sql;

use crate::{
    Result, draft,
    error::{ErrorKind, raise::stored},
    types::{Candidate, LibraryCard, LibraryRevision, Requirements},
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use jiff::Timestamp;

const CONTEXT: &str = "native Library operation";
const TENANT: &str = "tenant";
/// The visibility a published platform entry carries; draft creation reads it too.
pub(crate) const PLATFORM: &str = "platform";
const PLATFORM_DRAFT: &str = "platform_draft";

struct Entry {
    id: Uuid7,
    platform_slug: Option<String>,
    name: String,
    description: String,
    credential_reasons: Option<serde_json::Value>,
    updated_at: Timestamp,
    visibility: String,
    head_revision_id: Uuid7,
    resource_version: i64,
    created_at: Timestamp,
}

/// One `authoring.library_entries` row, by column name.
#[derive(sqlx::FromRow)]
struct EntryRow {
    id: String,
    platform_slug: Option<String>,
    name: String,
    description: String,
    credential_reasons: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    visibility: String,
    head_revision_id: Option<String>,
    resource_version: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

impl EntryRow {
    /// Every entry is written with its head in one transaction, so a headless
    /// row is a stored fault.
    fn decode(self) -> Result<Entry> {
        Ok(Entry {
            id: Uuid7::parse(&self.id)?,
            platform_slug: self.platform_slug,
            name: self.name,
            description: self.description,
            credential_reasons: self
                .credential_reasons
                .as_deref()
                .map(draft::value)
                .transpose()?,
            updated_at: self.updated_at,
            visibility: self.visibility,
            head_revision_id: Uuid7::parse(
                self.head_revision_id.as_deref().ok_or(ErrorKind::Hidden)?,
            )?,
            resource_version: self.resource_version,
            created_at: self.created_at,
        })
    }
}

impl Entry {
    fn public_id(&self) -> String {
        self.platform_slug
            .clone()
            .unwrap_or_else(|| self.id.to_string())
    }

    fn etag(&self) -> String {
        draft::etag(&self.id, self.resource_version)
    }

    fn card(&self, head: &Revision) -> LibraryCard {
        LibraryCard {
            id: self.public_id(),
            name: self.name.clone(),
            description: self.description.clone(),
            visibility: self.visibility.clone(),
            created_at: self.created_at,
            requirements: Requirements::of(&head.definition),
            required_credentials_reasons: self.credential_reasons.clone().unwrap_or_else(|| {
                head.definition.config().value()["x-agentsfleet"]
                    .get("required_credentials_reasons")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}))
            }),
            head_revision_id: self.head_revision_id.clone(),
            etag: self.etag(),
        }
    }
}

#[expect(
    clippy::struct_field_names,
    reason = "`revision` is the `authoring.library_revisions` column it decodes"
)]
pub(crate) struct Revision {
    pub(crate) id: Uuid7,
    pub(crate) revision: i64,
    pub(crate) definition: Definition,
    pub(crate) canonical_digest: String,
    pub(crate) policy_semantics: String,
    created_at: Timestamp,
}

/// One `authoring.library_revisions` row, by column name.
#[derive(sqlx::FromRow)]
struct RevisionRow {
    id: String,
    revision: i64,
    definition: String,
    canonical_digest: String,
    policy_semantics: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

impl RevisionRow {
    fn decode(self) -> Result<Revision> {
        Ok(Revision {
            id: Uuid7::parse(&self.id)?,
            revision: self.revision,
            definition: serde_json::from_str(&self.definition).map_err(stored)?,
            canonical_digest: self.canonical_digest,
            policy_semantics: self.policy_semantics,
            created_at: self.created_at,
        })
    }
}

impl Revision {
    pub(crate) fn candidate(&self) -> Candidate {
        Candidate {
            id: self.id.clone(),
            revision: self.revision.to_string(),
            definition: self.definition.clone(),
            canonical_digest: self.canonical_digest.clone(),
            policy_semantics: self.policy_semantics.clone(),
        }
    }

    fn project(self, library_id: String) -> LibraryRevision {
        LibraryRevision {
            id: self.id,
            library_id,
            revision: self.revision.to_string(),
            requirements: Requirements::of(&self.definition),
            definition: self.definition,
            canonical_digest: self.canonical_digest,
            policy_semantics: self.policy_semantics,
            created_at: self.created_at,
        }
    }
}
