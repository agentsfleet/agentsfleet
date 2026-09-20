//! Who the caller is — the payload `GET /v1/users/me` answers with.
//!
//! # Every field is derived, none is echoed
//!
//! A caller sends nothing to this route. The identifiers, the email and the
//! tenant name come from the row the authenticator's proven subject names; the
//! credential class comes from the principal the guard built. There is no field
//! here a request could influence, which is what makes the shape safe to hand
//! back under any of the three person credentials.
//!
//! # `display_name` is omitted rather than null
//!
//! The divergence from [`super::tenant`]'s always-emit rule is deliberate and
//! narrow. That rule exists for byte equality with a Zig emitter this route
//! never had, and the guidelines ask new surfaces to omit an absent optional
//! and keep `null` for a value somebody explicitly cleared. Nobody clears a
//! display name here — it was either supplied at signup or never was — so the
//! key is absent, matching [`super::tenant_model_entry`], the other surface
//! with no Zig original.
//!
//! # No credential material, structurally
//!
//! There is no field on this struct that could carry a token, a digest or a
//! credential prefix, and there is no sibling shape in this module that could
//! either. That is the same argument [`super::tenant`] makes about its mint
//! reply, run the other way: a response shape with nowhere to put a secret
//! cannot leak one however the statement behind it changes.

use std::borrow::Cow;

use serde::Serialize;

/// The wire spellings of the three credential classes a person can act through.
///
/// Snake case, matching every other discriminant this crate emits, and named
/// once here because the daemon renders them and clients branch on them —
/// two ends of one fact.
pub mod credential_class {
    /// A browser session token, verified against the provider's key set.
    pub const SESSION_TOKEN: &str = "session_token";
    /// An `agt_t` key, resolving to the person who created it.
    pub const TENANT_API_KEY: &str = "tenant_api_key";
    /// An `afc_` credential minted by `agentsfleet login`.
    pub const CLI_CREDENTIAL: &str = "cli_credential";
}

/// `GET /v1/users/me` — the person, the tenant, and how they proved it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentUserResponse<'a> {
    /// Your own identifier. Stable for the life of the account.
    //
    // Not the identity provider's subject, which is deliberately absent from
    // this shape: it identifies the caller to a third party and answers no
    // question a client of ours has. A `//` comment, not a doc comment, because
    // that reasoning is ours and the published field description is the reader's.
    pub user_id: Cow<'a, str>,
    /// The address this account was opened with.
    pub email: Cow<'a, str>,
    /// Your display name. Absent when the account never set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<Cow<'a, str>>,
    /// The tenant you act in.
    pub tenant_id: Cow<'a, str>,
    /// That tenant's display name.
    pub tenant_name: Cow<'a, str>,
    /// How you signed in: `session_token`, `tenant_api_key`, or `cli_credential`.
    pub credential: Cow<'a, str>,
    /// What you may do. Each entry is a scope name, such as `fleet:read`.
    ///
    /// An empty list means you hold no scopes. The key is always present.
    //
    // Always emitted, never omitted, so "none" and "not answered" stay
    // distinguishable: a person who can do nothing still reads their identity.
    pub scopes: Vec<Cow<'a, str>>,
}
