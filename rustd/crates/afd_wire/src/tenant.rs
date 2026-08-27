//! The tenant plane's payloads: api-keys, and the credentials beside them.
//!
//! # The mint reply is the only place a secret appears
//!
//! [`MintedApiKeyResponse`] carries `key`; nothing else in this module has a
//! field that could. That is structural rather than careful — a list shape with
//! no secret field cannot leak one however the statement behind it changes —
//! and it is half of what "revealed exactly once" means. The other half is
//! `afd_fleet::credential::minted::Minted`, which zeroes the plaintext when the
//! response holding it is done.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// `POST /v1/api-keys` — mint one tenant credential.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintApiKeyRequest<'a> {
    /// What the key will be called. Unique per tenant.
    #[serde(borrow)]
    pub key_name: Cow<'a, str>,
    /// Free text beside it, for whoever reads the list in six months.
    #[serde(borrow, default)]
    pub description: Option<Cow<'a, str>>,
}

/// The one response that reveals a key's plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MintedApiKeyResponse<'a> {
    /// The key's identifier, which every later call addresses it by.
    pub id: Cow<'a, str>,
    /// What it is called.
    pub key_name: Cow<'a, str>,
    /// The credential itself. Shown once and never stored in this shape.
    pub key: Cow<'a, str>,
    /// When it was minted.
    pub created_at: i64,
}

/// `POST /v1/cli-credentials` — mint this machine's credential.
///
/// Unknown fields are IGNORED, where [`MintApiKeyRequest`] beside it denies
/// them. Not an oversight and not a style drift: `cli_credentials.zig` parses
/// with `.ignore_unknown_fields = true`, and a client that has been sending a
/// field this daemon does not read would start receiving a 400 the moment an
/// attribute were added here for tidiness. Serde ignores by default, so the
/// parity is kept by the ABSENCE of an attribute — which is exactly why it is
/// written down.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MintCliCredentialRequest<'a> {
    /// The terminal's own label, as an operator will read it back.
    #[serde(borrow)]
    pub machine_name: Cow<'a, str>,
}

/// The one response that reveals a command-line credential's plaintext.
///
/// Carries `credential` for the reason [`MintedApiKeyResponse`] carries `key`,
/// and nothing else on this surface has a field that could — there is no list
/// verb here at all, so the mint reply is the whole of the exposure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MintedCliCredentialResponse<'a> {
    /// The credential row's identifier, which the revoke addresses it by.
    pub id: Cow<'a, str>,
    /// The credential itself. Shown once and never stored in this shape.
    pub credential: Cow<'a, str>,
    /// The terminal's label, echoed back as it was stored.
    pub machine_name: Cow<'a, str>,
    /// The deployment that minted it — this daemon, never a caller's claim.
    pub deployment: Cow<'a, str>,
}

/// `PATCH /v1/api-keys/{id}` — the only mutation, and only downward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchApiKeyRequest {
    /// Must be `false`. A revoked key is never brought back.
    pub active: bool,
}

/// What revoking answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevokedApiKeyResponse<'a> {
    /// The key's identifier.
    pub id: Cow<'a, str>,
    /// Always `false` — the field is present so a client can write the reply
    /// straight into whatever it was holding.
    pub active: bool,
    /// When the row records it stopped working.
    pub revoked_at: i64,
}

/// One key as a list shows it: metadata, never a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiKeySummary<'a> {
    /// The key's identifier.
    pub id: Cow<'a, str>,
    /// What it is called.
    pub key_name: Cow<'a, str>,
    /// Whether it still authenticates.
    pub active: bool,
    /// When it was minted.
    pub created_at: i64,
    /// When it last authenticated, if it ever has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
    /// When it stopped working, if it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<i64>,
}

/// A page of a keyset-paginated list, in the envelope every list shares.
///
/// Generic over the item, because the envelope is the same for every paged
/// resource on these planes and a per-resource copy is a per-resource chance
/// for `has_more` to be computed differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageResponse<'a, T> {
    /// The rows on this page, in the requested order.
    pub data: Vec<T>,
    /// Whether another page exists.
    ///
    /// Derived from the page being FULL rather than from the total, because the
    /// total is page-stable and a row deleted mid-walk would otherwise make it
    /// disagree with what the cursor can actually reach.
    pub has_more: bool,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Cow<'a, str>>,
    /// How many rows the collection holds in total.
    pub total: i64,
}
