//! The people in an account, on the wire: invites into it, the members who
//! accepted, and the members a workspace's thread names.
//!
//! Lists ride [`crate::tenant::PageResponse`], the envelope every list on this
//! API shares. An account's invites and members are few, so each list is one
//! page: `next_cursor` is always `null` and `total` is the whole count.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::workspace::WorkspaceAccount;

/// `POST /v1/tenants/me/invites` — invite one address into the caller's account.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInviteRequest<'a> {
    /// The address to invite. Stored lowercased; accepting requires the
    /// signed-in account's address to equal it. A domain literal, an IP
    /// domain, a domain without a dot, or over 254 characters is refused
    /// with 400 `UZ-REQ-001`.
    #[serde(borrow)]
    pub email: Cow<'a, str>,
}

/// One invite, as the account's owner sees it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InviteSummary<'a> {
    /// The invite's identifier.
    pub id: Cow<'a, str>,
    /// The address it is for, lowercased.
    pub email: Cow<'a, str>,
    /// The role accepting it grants: `member`.
    pub role: Cow<'a, str>,
    /// When it stops being acceptable, epoch milliseconds.
    pub expires_at: i64,
    /// When it was issued, epoch milliseconds.
    pub created_at: i64,
    /// The dashboard page the invitee opens to accept it.
    pub link: Cow<'a, str>,
    /// What became of its most recent email: `sent`, `failed`, or
    /// `unconfigured` when this deployment has no mail relay set up.
    pub email_status: Cow<'a, str>,
    /// When the relay last accepted its email, epoch milliseconds; `null` if
    /// it never has.
    pub email_sent_at: Option<i64>,
}

/// `POST /v1/tenants/me/invites/{invite_id}/send` — the invite email went.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InviteEmailResponse<'a> {
    /// `sent`: the relay accepted the email.
    pub email_status: Cow<'a, str>,
}

/// One invite waiting for the caller's address.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaitingInvite<'a> {
    /// The invite's identifier, which the accept route names.
    pub id: Cow<'a, str>,
    /// The account accepting it joins.
    pub account: WorkspaceAccount<'a>,
    /// When it stops being acceptable, epoch milliseconds.
    pub expires_at: i64,
}

/// `POST /v1/users/me/invites/{invite_id}/accept` — the account joined.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptedInviteResponse<'a> {
    /// The account the caller is now a member of.
    pub tenant_id: Cow<'a, str>,
    /// Its workspaces, oldest first, each now open to the caller.
    pub workspace_ids: Vec<Cow<'a, str>>,
}

/// One member of the caller's account.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemberSummary<'a> {
    /// The member's user identifier.
    pub user_id: Cow<'a, str>,
    /// Their name, `null` when the identity provider supplied none.
    pub display_name: Option<Cow<'a, str>>,
    /// Their address.
    pub email: Cow<'a, str>,
    /// Their role: `owner` or `member`.
    pub role: Cow<'a, str>,
    /// When they joined the account, epoch milliseconds.
    pub joined_at: i64,
}

/// One member of a workspace's account, as its thread names senders.
///
/// No address: every member of an account can read this list, and a name is
/// all a thread needs.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceMember<'a> {
    /// The member's user identifier.
    pub user_id: Cow<'a, str>,
    /// Their name, `null` when the identity provider supplied none.
    pub display_name: Option<Cow<'a, str>>,
    /// Their role: `owner` or `member`.
    pub role: Cow<'a, str>,
    /// The actor their messages record, `steer:<subject>`: what a thread
    /// row's `actor` equals when this member sent it.
    pub actor: Cow<'a, str>,
}
