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
    draft,
    types::{Candidate, LibraryCard, LibraryRevision, Requirements},
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use serde::Deserialize;

const CONTEXT: &str = "native Library operation";
const TENANT: &str = "tenant";
/// The visibility a published platform entry carries; draft creation reads it too.
pub(crate) const PLATFORM: &str = "platform";
const PLATFORM_DRAFT: &str = "platform_draft";

#[derive(Deserialize)]
struct Entry {
    id: Uuid7,
    platform_slug: Option<String>,
    name: String,
    description: String,
    credential_reasons: Option<serde_json::Value>,
    updated_at: i64,
    visibility: String,
    head_revision_id: Uuid7,
    resource_version: i64,
    pub(super) created_at: i64,
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
    reason = "`revision` is the `core.fleet_library_revisions` column, decoded from `to_jsonb(r)`"
)]
#[derive(Deserialize)]
pub(crate) struct Revision {
    pub(crate) id: Uuid7,
    pub(crate) revision: i64,
    pub(crate) definition: Definition,
    pub(crate) canonical_digest: String,
    pub(crate) policy_semantics: String,
    created_at: i64,
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
