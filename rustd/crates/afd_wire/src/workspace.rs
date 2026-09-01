//! The workspace directory's payloads: the tenant's list, and the create.
//!
//! # The list envelope is the shared one plus a key, by design
//!
//! [`WorkspacesResponse`] carries `tenant_id` beside `items` — the one
//! security-bound exception `docs/REST_API_DESIGN_GUIDELINES.md` grants:
//! browser and command-line clients persist the authoritative tenant with the
//! workspace list, so a refreshed identity cannot mix local state from two
//! tenants. `total` is always `null` here — `tenant_workspaces.zig` never
//! counts — and stays on the wire anyway, because removing a key a client can
//! see is a shape change.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// `POST /v1/workspaces` — create one.
///
/// Unknown fields are IGNORED, like the command-line credential mint and for
/// its reason: `lifecycle.zig` parses with `.ignore_unknown_fields = true`,
/// and the parity is kept by the ABSENCE of a serde attribute. `name` is
/// optional twice over — absent, `null`, or blank all mean "name it for me".
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct CreateWorkspaceRequest<'a> {
    /// What the workspace will be called, when the caller cares.
    #[serde(borrow, default)]
    pub name: Option<Cow<'a, str>>,
}

/// What creating answers with.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreatedWorkspaceResponse<'a> {
    /// The new workspace's identifier.
    pub workspace_id: Cow<'a, str>,
    /// Its name — echoed when chosen, revealed when generated.
    pub name: Cow<'a, str>,
    /// The correlation token, in the body as `lifecycle.zig` writes it.
    pub request_id: Cow<'a, str>,
    /// The tenant it was created in — the daemon's resolution, never a claim.
    pub tenant_id: Cow<'a, str>,
}

/// One workspace as the list shows it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceSummary<'a> {
    /// The workspace's identifier.
    pub id: Cow<'a, str>,
    /// Its name — `null` on rows older than naming, emitted either way for
    /// the tenant module's null rule.
    pub name: Option<Cow<'a, str>>,
    /// When it was created; the walk's sort key.
    pub created_at: i64,
}

/// `GET /v1/tenants/me/workspaces` — one page of the tenant's workspaces.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspacesResponse<'a> {
    /// The rows on this page, oldest first.
    pub items: Vec<WorkspaceSummary<'a>>,
    /// Whose list this is — the authoritative resolution, carried so a client
    /// can pin its local state to the right tenant.
    pub tenant_id: Cow<'a, str>,
    /// Always `null`: the walk never counts, and the key stays because a
    /// client may already branch on its presence.
    pub total: Option<i64>,
    /// Where the next page resumes, or `null` on the last page.
    pub next_cursor: Option<Cow<'a, str>>,
}
