//! Explicit publication copies native candidates while keeping drafts private.

mod history;
mod list;
mod publish;
mod read;
mod remove;
mod source;
mod sql;

use crate::{
    Result, draft,
    error::{ErrorKind, raise::stored},
    types::{Candidate, PluginCard, PluginRevision, Requirements},
};
use af_core::id::Uuid7;
use af_fleet_runtime::Definition;
use jiff::Timestamp;

const CONTEXT: &str = "native Plugin operation";

struct Entry {
    id: Uuid7,
    name: String,
    description: String,
    credential_reasons: Option<serde_json::Value>,
    head_revision_id: Uuid7,
    resource_version: i64,
    created_at: Timestamp,
}

/// One `authoring.plugin_entries` row, by column name.
#[derive(sqlx::FromRow)]
struct EntryRow {
    id: Uuid7,
    name: String,
    description: String,
    credential_reasons: Option<String>,
    head_revision_id: Option<Uuid7>,
    resource_version: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

impl EntryRow {
    /// Every entry is written with its head in one transaction, so a headless
    /// row is a stored fault.
    fn decode(self) -> Result<Entry> {
        // A card without a head revision is not yet readable as a Plugin entry.
        let head = self.head_revision_id.as_ref().ok_or(ErrorKind::Hidden)?;
        Ok(Entry {
            id: self.id.clone(),
            name: self.name,
            description: self.description,
            credential_reasons: self
                .credential_reasons
                .as_deref()
                .map(draft::value)
                .transpose()?,
            head_revision_id: Uuid7::parse(head.as_str())?,
            resource_version: self.resource_version,
            created_at: self.created_at,
        })
    }
}

impl Entry {
    fn etag(&self) -> String {
        draft::etag(&self.id, self.resource_version)
    }

    fn card(&self, head: &Revision) -> PluginCard {
        PluginCard {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
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
    reason = "`revision` is the `authoring.plugin_revisions` column it decodes"
)]
pub(crate) struct Revision {
    pub(crate) id: Uuid7,
    pub(crate) revision: i64,
    pub(crate) definition: Definition,
    pub(crate) canonical_digest: String,
    pub(crate) policy_semantics: String,
    created_at: Timestamp,
}

/// One `authoring.plugin_revisions` row, by column name.
#[derive(sqlx::FromRow)]
struct RevisionRow {
    id: Uuid7,
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
            id: self.id.clone(),
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

    fn project(self, plugin_id: Uuid7) -> PluginRevision {
        PluginRevision {
            id: self.id,
            plugin_id,
            revision: self.revision.to_string(),
            requirements: Requirements::of(&self.definition),
            definition: self.definition,
            canonical_digest: self.canonical_digest,
            policy_semantics: self.policy_semantics,
            created_at: self.created_at,
        }
    }
}
