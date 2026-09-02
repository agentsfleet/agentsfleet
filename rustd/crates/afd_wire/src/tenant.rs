//! The tenant plane's payloads: api-keys, and the credentials beside them.
//!
//! # The mint reply is the only place a secret appears
//!
//! [`MintedApiKeyResponse`] carries `key`; nothing else in this module has a
//! field that could. That is structural rather than careful — a list shape with
//! no secret field cannot leak one however the statement behind it changes —
//! and it is half of what "revealed exactly once" means. The other half is
//! `afd_auth::minted::Minted`, which zeroes the plaintext when the
//! response holding it is done.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// `POST /v1/api-keys` — mint one tenant credential.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
//
// Unknown fields are IGNORED, where [`MintApiKeyRequest`] beside it denies
// them. Not an oversight and not a style drift: `cli_credentials.zig` parses
// with `.ignore_unknown_fields = true`, and a client that has been sending a
// field this daemon does not read would start receiving a 400 the moment an
// attribute were added here for tidiness. Serde ignores by default, so the
// parity is kept by the ABSENCE of an attribute — which is exactly why it is
// written down.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchApiKeyRequest {
    /// Must be `false`. A revoked key is never brought back.
    pub active: bool,
}

/// What revoking answers with.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    pub last_used_at: Option<i64>,
    /// When it stopped working, if it has.
    ///
    /// Emitted as `null` rather than omitted — here and on every optional in
    /// this module. The Zig daemon serialises through `res.json(value, .{})`,
    /// and std.json's default emits null optionals, so a row is always the
    /// same set of keys. Omission would be a shape change a dashboard's
    /// `"revoked_at" in row` check can feel.
    pub revoked_at: Option<i64>,
}

/// `GET /v1/tenants/me/billing` — the wallet snapshot.
//
// `is_exhausted` restates `exhausted_at` as a boolean, and both travel:
// `tenant_billing.zig` emits the pair so a dashboard can branch without a
// null-check, and parity keeps the redundancy. `updated_at` and
// `exhausted_at` are instants in milliseconds; the `_ms` suffix the domain
// types carry stops at the wire because the Zig field names are the format.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BillingResponse {
    /// What remains, in nanos — one thousand-millionth of a dollar.
    pub balance_nanos: i64,
    /// When the balance last moved.
    pub updated_at: i64,
    /// Whether the balance has reached zero.
    pub is_exhausted: bool,
    /// When it did, or `null` while money remains — emitted either way, for
    /// the module's null rule.
    pub exhausted_at: Option<i64>,
}

/// One ledger row as the charges list shows it.
///
/// Field-for-field the Zig `TelemetryRow`, in its order — the struct is
/// serialized straight to JSON there, so the row IS the wire shape.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChargeSummary<'a> {
    /// The ledger row's identifier.
    pub id: Cow<'a, str>,
    /// Whose charge it is.
    pub tenant_id: Cow<'a, str>,
    /// The workspace it was incurred in — `null` once that workspace is
    /// deleted, because a charge outlives what it was incurred on.
    pub workspace_id: Option<Cow<'a, str>>,
    /// The fleet it was incurred by, under the same deletion rule.
    pub fleet_id: Option<Cow<'a, str>>,
    /// The event that triggered the work.
    pub event_id: Cow<'a, str>,
    /// `receive` or `stage` — the two halves of one event's cost.
    pub charge_type: Cow<'a, str>,
    /// Whose model bill the tokens landed on.
    pub posture: Cow<'a, str>,
    /// The model the stage ran against.
    pub model: Cow<'a, str>,
    /// What this row drained, in nanos.
    pub credit_deducted_nanos: i64,
    /// Tokens in, on stage rows that have settled.
    pub token_count_input: Option<i64>,
    /// Tokens out, likewise.
    pub token_count_output: Option<i64>,
    /// How long the stage ran.
    pub wall_ms: Option<i64>,
    /// When the row was written — the walk's sort key, and what its cursor
    /// half is rendered from.
    pub recorded_at: i64,
}

/// `GET /v1/tenants/me/billing/charges` — one page of the ledger.
//
// Two keys, not [`PageResponse`]'s three: the Zig handler answers
// `{items, next_cursor}` with no `total`, and parity pins the ABSENCE the
// same way it pins a presence. Folding this into the shared envelope would
// put a key on the wire the daemon being replaced never sent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChargesResponse<'a> {
    /// The rows on this page, newest first.
    pub items: Vec<ChargeSummary<'a>>,
    /// Where the next page resumes, or `null` on the last page — emitted
    /// either way, for the module's null rule.
    pub next_cursor: Option<Cow<'a, str>>,
}

/// A page of a keyset-paginated list, in the envelope every list shares.
///
/// Generic over the item, because the envelope is the same for every paged
/// resource on these planes and a per-resource copy is a per-resource chance
/// for the keys to drift apart.
///
/// Exactly three keys, always — `items`, `total`, `next_cursor` — pinned to
/// what `api_keys/list.zig` answers and to the integration test that counts
/// them. An earlier shape here said `data` with a `has_more` beside it, which
/// read well and was nobody's wire format: parity is with the daemon being
/// replaced, not with the envelope one would design today.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageResponse<'a, T> {
    /// The rows on this page, in the requested order.
    pub items: Vec<T>,
    /// How many rows the collection holds in total.
    ///
    /// Page-stable — the count carries no keyset predicate — so a client
    /// walking pages sees one number rather than a shrinking one. Whether more
    /// pages exist is answered by `next_cursor` being non-null, not by a flag.
    pub total: i64,
    /// Where the next page resumes, or `null` on the last page.
    ///
    /// Always emitted, never omitted, for the module's null rule.
    pub next_cursor: Option<Cow<'a, str>>,
}
