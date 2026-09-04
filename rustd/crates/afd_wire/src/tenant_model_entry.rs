//! `/v1/tenants/me/models` — the registry a tenant chooses its provider from.
//!
//! [`crate::tenant_provider`] carries the ONE selection a tenant's runs resolve
//! through. This is the list it is chosen from, and the two are read together:
//! a row's `active` says whether it IS the current selection, and the page's
//! `platform_default` says what the alternative looks like.
//!
//! # Nothing here carries key material, in either direction
//!
//! A row names its credential (`secret_ref`) and reports whether one is stored
//! (`has_key`). It never carries the key, and there is no field it would fit
//! in — the same guarantee `vault.secrets`' projection columns have, since
//! every field below is read out of those columns rather than out of the
//! ciphertext beside them.
//!
//! # Two nulls stay on the wire, and the rest are omitted
//!
//! The Zig serializes this page with `emit_null_optional_fields = false`, so an
//! absent `provider` or `base_url` is omitted rather than sent as null — that
//! is the shape the dashboard's union narrows on. `total` and `next_cursor` are
//! the exception: `docs/REST_API_DESIGN_GUIDELINES.md` §3 requires both to be
//! PRESENT on every page including the last, so they are always emitted.
//!
//! `total` is always null, and that is not a gap. Counting a keyset page costs
//! the scan the pagination exists to avoid, and §3 declares null to mean "not
//! computed" rather than letting the key vanish.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The longest model identifier this surface accepts.
///
/// One rule and two request types, so the bound cannot hold on the create and
/// not on the change — which is exactly how `model_id` ended up bounded on the
/// catalogue route and unbounded on this one. The refusal sentence spells the
/// same number in words; both move together or neither does.
pub const MODEL_ID_MAX_BYTES: usize = 256;

/// One configured model, as the registry page shows it.
///
/// `id` is the entry's own identity — the `{id}` the item route takes — and not
/// the model's. The model is `model_id`, which is what a provider spells.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelEntryRow<'a> {
    /// The entry's identity.
    pub id: &'a str,
    /// The model this entry configures.
    pub model_id: &'a str,
    /// The vault key name backing it.
    pub secret_ref: &'a str,
    /// The provider label, for the credential kinds that carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<&'a str>,
    /// What the credential is, as the server classified it at write time.
    pub kind: &'a str,
    /// The custom endpoint, where one may be displayed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<&'a str>,
    /// Whether a key is stored under that name. Never the key.
    pub has_key: bool,
    // A blank cell, which is different from a zero window and must not render
    // as one.
    /// The context window this model is priced at, in tokens.
    ///
    /// Absent when the catalogue carries no rate for this model. An absent
    /// value does not mean zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_cap_tokens: Option<u32>,
    /// The input rate, in nanos per million tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_nanos_per_mtok: Option<i64>,
    /// The cached-input rate, likewise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_nanos_per_mtok: Option<i64>,
    /// The output rate, likewise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_nanos_per_mtok: Option<i64>,
    /// Whether this entry is the tenant's current selection.
    pub active: bool,
    /// When it was first stored, in epoch milliseconds.
    pub created_at: i64,
}

/// The deployment's platform default, as the page's Default row shows it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformDefaultRow<'a> {
    /// The provider the default belongs to.
    pub provider: &'a str,
    /// The model it selects.
    pub model: &'a str,
    /// The context window the catalogue prices it at.
    pub context_cap_tokens: u32,
    /// The input rate, in nanos per million tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_nanos_per_mtok: Option<i64>,
    /// The cached-input rate, likewise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_nanos_per_mtok: Option<i64>,
    /// The output rate, likewise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_nanos_per_mtok: Option<i64>,
}

/// `GET /v1/tenants/me/models` — one page of the registry.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelEntriesResponse<'a> {
    // `models`, not `items`: renaming a shipped v1 field is what
    // `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids. `total` and
    // `next_cursor` were ADDED beside it, so the page became navigable without
    // breaking a client.
    /// The registered models, newest first.
    pub models: Vec<ModelEntryRow<'a>>,
    /// Always null — see the module note.
    pub total: Option<u64>,
    /// Where the next page resumes, or null on the last one.
    pub next_cursor: Option<String>,
    /// Whether an active platform-default row exists at all.
    //
    // Derived from the same read as `platform_default`, so the two cannot
    // disagree about whether there is one.
    pub platform_default_available: bool,
    /// The default's identity, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_default: Option<PlatformDefaultRow<'a>>,
}

/// What a create and a model change both answer with.
///
/// One shape for both verbs because both say the same thing — which entry now
/// stands — and the status code is what distinguishes them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredModelEntry<'a> {
    /// The entry's identity.
    pub id: &'a str,
    /// The model it configures.
    pub model_id: &'a str,
    /// The vault key name backing it.
    pub secret_ref: &'a str,
    /// When it was first stored, in epoch milliseconds.
    pub created_at: i64,
}

// Unknown fields are IGNORED, matching `innerCreateModelEntry`'s
// `.ignore_unknown_fields = true`, and the parity is kept by the ABSENCE of a
// serde attribute.
/// Registers a model against a stored credential.
///
/// agentsfleet ignores fields it does not know, instead of refusing the
/// request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateModelEntryRequest {
    /// The model to register.
    #[garde(length(bytes, min = 1, max = MODEL_ID_MAX_BYTES))]
    pub model_id: String,
    /// The credential to back it with. Immutable once the entry exists.
    // Bounded below only: the ceiling belongs to the vault's key name, which
    // refuses a reference no secret could carry — a length here would be a
    // second opinion about someone else's column.
    #[garde(length(bytes, min = 1))]
    pub secret_ref: String,
}

/// `PATCH /v1/tenants/me/models/{id}` — point an entry at another model.
//
// There is no `secret_ref` field and adding one would be a different verb: the
// same model on a different credential is a DIFFERENT entry, which is what the
// table's domain key says.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateModelEntryRequest {
    /// The model to point at.
    #[garde(length(bytes, min = 1, max = MODEL_ID_MAX_BYTES))]
    pub model_id: String,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{ModelEntriesResponse, ModelEntryRow, PlatformDefaultRow};

    /// A row with nothing the catalogue or the vault could tell us about.
    fn bare() -> ModelEntryRow<'static> {
        ModelEntryRow {
            id: "0195b4ba-8d3a-7f13-8abc-cd0000000002",
            model_id: "claude-opus-5",
            secret_ref: "anthropic-prod",
            provider: None,
            kind: "custom_secret",
            base_url: None,
            has_key: false,
            context_cap_tokens: None,
            input_nanos_per_mtok: None,
            cached_input_nanos_per_mtok: None,
            output_nanos_per_mtok: None,
            active: false,
            created_at: 1_777_507_200_000,
        }
    }

    #[test]
    fn a_row_the_vault_cannot_describe_carries_no_descriptor_keys_at_all() {
        // The dashboard narrows on which keys are PRESENT, so a `provider:
        // null` would be a field the union says that variant does not have.
        // This is the row whose credential was deleted out of band.
        let body = serde_json::to_string(&bare()).expect("the row serializes");

        assert_eq!(
            body,
            r#"{"id":"0195b4ba-8d3a-7f13-8abc-cd0000000002","model_id":"claude-opus-5","secret_ref":"anthropic-prod","kind":"custom_secret","has_key":false,"active":false,"created_at":1777507200000}"#
        );
    }

    #[test]
    fn an_empty_page_still_carries_both_navigation_keys() {
        // §3 requires `total` and `next_cursor` to be present on EVERY page
        // including the last, so a client can read them without branching on
        // their absence first. They are the two nulls that stay on this wire.
        let body = serde_json::to_string(&ModelEntriesResponse {
            models: vec![],
            total: None,
            next_cursor: None,
            platform_default_available: false,
            platform_default: None,
        })
        .expect("the page serializes");

        assert_eq!(
            body,
            r#"{"models":[],"total":null,"next_cursor":null,"platform_default_available":false}"#
        );
    }

    #[test]
    fn a_configured_default_rides_beside_the_flag_that_announces_it() {
        let body = serde_json::to_string(&ModelEntriesResponse {
            models: vec![],
            total: None,
            next_cursor: Some("token".to_owned()),
            platform_default_available: true,
            platform_default: Some(PlatformDefaultRow {
                provider: "anthropic",
                model: "claude-sonnet-5",
                context_cap_tokens: 200_000,
                input_nanos_per_mtok: Some(3_000_000),
                cached_input_nanos_per_mtok: None,
                output_nanos_per_mtok: None,
            }),
        })
        .expect("the page serializes");

        assert_eq!(
            body,
            r#"{"models":[],"total":null,"next_cursor":"token","platform_default_available":true,"platform_default":{"provider":"anthropic","model":"claude-sonnet-5","context_cap_tokens":200000,"input_nanos_per_mtok":3000000}}"#
        );
    }

    #[test]
    fn a_create_body_ignores_fields_this_daemon_does_not_know() {
        // `.ignore_unknown_fields = true` in the Zig, and the parity is the
        // ABSENCE of `deny_unknown_fields` here — a dashboard sending a field a
        // newer build understands must not be refused by an older one.
        let parsed: super::CreateModelEntryRequest = serde_json::from_str(
            r#"{"model_id":"claude-opus-5","secret_ref":"anthropic-prod","nickname":"prod"}"#,
        )
        .expect("an unknown field is ignored");

        assert_eq!(parsed.model_id, "claude-opus-5");
        assert_eq!(parsed.secret_ref, "anthropic-prod");
    }
}
