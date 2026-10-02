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
//
// Unknown fields are IGNORED, like the command-line credential mint and for
// its reason: `lifecycle.zig` parses with `.ignore_unknown_fields = true`,
// and the parity is kept by the ABSENCE of a serde attribute. `name` is
// optional twice over — absent, `null`, or blank all mean "name it for me".
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
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
    // In the body as `lifecycle.zig` writes it.
    /// The correlation token for this request, repeated in the body.
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
    /// The account it belongs to, which is how a dashboard groups the list.
    pub account: WorkspaceAccount<'a>,
    /// The caller's role in that account: `owner` or `member`.
    pub role: Cow<'a, str>,
}

/// The account a listed workspace belongs to.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceAccount<'a> {
    /// The account's tenant.
    pub tenant_id: Cow<'a, str>,
    /// What a person calls the account: its owner's display name, or the
    /// account's own name when the owner has none.
    pub owner_name: Cow<'a, str>,
}

/// `GET /v1/tenants/me/workspaces` — one page of the workspaces across every
/// account the caller holds.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspacesResponse<'a> {
    /// The rows on this page, oldest first.
    pub items: Vec<WorkspaceSummary<'a>>,
    /// The caller's own account — the authoritative resolution, carried so a
    /// client can pin its local state to the right person. Items from accounts
    /// the caller joined name theirs in `account`.
    pub tenant_id: Cow<'a, str>,
    /// Always `null`: the walk never counts, and the key stays because a
    /// client may already branch on its presence.
    pub total: Option<i64>,
    /// Where the next page resumes, or `null` on the last page.
    pub next_cursor: Option<Cow<'a, str>>,
}
