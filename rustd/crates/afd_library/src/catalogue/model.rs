//! Domain values for the platform Fleet-library catalogue.

use afd_core::clock::UnixMillis;
use serde_json::Value;

/// Requirement names projected without bundle content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRequirements {
    credentials: Vec<String>,
    tools: Vec<String>,
    network_hosts: Vec<String>,
    trigger_present: bool,
}

impl LibraryRequirements {
    pub(super) const fn new(
        credentials: Vec<String>,
        tools: Vec<String>,
        network_hosts: Vec<String>,
        trigger_present: bool,
    ) -> Self {
        Self {
            credentials,
            tools,
            network_hosts,
            trigger_present,
        }
    }

    /// Required credential names only.
    #[must_use]
    pub fn credentials(&self) -> &[String] {
        &self.credentials
    }

    /// Required tool identifiers.
    #[must_use]
    pub fn tools(&self) -> &[String] {
        &self.tools
    }

    /// Declared outbound hosts.
    #[must_use]
    pub fn network_hosts(&self) -> &[String] {
        &self.network_hosts
    }

    /// Whether the bundle has a trigger document.
    #[must_use]
    pub const fn trigger_present(&self) -> bool {
        self.trigger_present
    }
}

/// One admin catalogue row; bundle bodies and object keys are not projected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryItem {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) source_repo: String,
    pub(super) source_ref: String,
    pub(super) visibility: String,
    pub(super) content_hash: Option<String>,
    pub(super) requirements: LibraryRequirements,
    pub(super) required_credentials_reasons: Value,
    pub(super) updated_at: UnixMillis,
    pub(super) etag: String,
}

impl LibraryItem {
    /// Slug identity from `SKILL.md`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Operator-visible name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Operator-curated description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// GitHub owner/repository.
    #[must_use]
    pub fn source_repo(&self) -> &str {
        &self.source_repo
    }

    /// Git branch, tag, or commit fetched.
    #[must_use]
    pub fn source_ref(&self) -> &str {
        &self.source_ref
    }

    /// Draft or public lifecycle state.
    #[must_use]
    pub fn visibility(&self) -> &str {
        &self.visibility
    }

    /// Immutable bundle identity when fetched.
    #[must_use]
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }

    /// Content-free requirement summary.
    #[must_use]
    pub const fn requirements(&self) -> &LibraryRequirements {
        &self.requirements
    }

    /// Operator-authored credential reason copy.
    #[must_use]
    pub const fn required_credentials_reasons(&self) -> &Value {
        &self.required_credentials_reasons
    }

    /// Last mutation instant.
    #[must_use]
    pub const fn updated_at(&self) -> UnixMillis {
        self.updated_at
    }

    /// Strong tag over the operator-editable surface.
    #[must_use]
    pub fn etag(&self) -> &str {
        &self.etag
    }
}

/// Curated fields accepted by an admin patch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryPatch {
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) source_repo: Option<String>,
    pub(super) source_ref: Option<String>,
    pub(super) required_credentials_reasons: Option<Value>,
    pub(super) published: Option<bool>,
}

impl LibraryPatch {
    /// Builds an already-validated partial update.
    #[must_use]
    pub const fn new(
        name: Option<String>,
        description: Option<String>,
        source_repo: Option<String>,
        source_ref: Option<String>,
        required_credentials_reasons: Option<Value>,
        published: Option<bool>,
    ) -> Self {
        Self {
            name,
            description,
            source_repo,
            source_ref,
            required_credentials_reasons,
            published,
        }
    }
}

/// Patch outcome kept distinct from datastore failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchLibrary {
    /// The updated row as it now stands.
    Updated(Box<LibraryItem>),
    /// No row has this catalogue id.
    NotFound,
    /// Publication was requested without a current bundle.
    PublishWithoutBundle,
    /// The caller's `If-Match` no longer names the row.
    Stale {
        /// Current tag the caller should rebase onto.
        etag: String,
    },
}

/// Delete outcome kept distinct from datastore failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteLibrary {
    /// The draft row was deleted.
    Deleted,
    /// No row has this catalogue id.
    NotFound,
    /// A public row must be withdrawn first.
    Published,
}
