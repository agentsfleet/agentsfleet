//! The workspace secret surface's payloads: create, list, replace.
//!
//! There is no response shape carrying a stored value, and no request shape
//! asking for one. A secret is write-only by contract — `SecretSummary` in
//! `public/openapi/` documents the descriptors and nothing else — so the
//! absence is the surface, not an omission.
//!
//! # The body rides as raw JSON, unparsed here
//!
//! `data` is `&RawValue`: this crate validates that it IS JSON and hands the
//! bytes on. Whether it is a non-empty OBJECT within its bound is
//! `afd_vault::SecretBody`'s question, asked once, in the constructor that also
//! produces the projection from that same parse. Deciding it here would mean
//! two parses of one secret and two places for the two answers to disagree.
//!
//! # Nulls do NOT stay on the wire here, and that is the documented shape
//!
//! Every other module in this crate serializes an absent optional as `null`,
//! because the Zig emitter does. The secret list is the exception:
//! `respondSecretList` passes `.emit_null_optional_fields = false` so each row
//! carries only its own kind's descriptors, and the dashboard's `Secret` union
//! narrows on exactly that — a `provider: null` on a `custom_secret` would be a
//! field the union says that variant does not have.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

// Unknown fields are IGNORED, matching `innerStoreSecret`'s
// `.ignore_unknown_fields = true`, and the parity is kept by the ABSENCE of a
// serde attribute.
/// Stores one secret under a name you choose.
///
/// agentsfleet ignores fields it does not know, instead of refusing the
/// request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize)]
pub struct StoreSecretRequest<'a> {
    /// The name a fleet interpolates as `${secrets.<name>.<field>}`.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    // The serialized form is a JSON object; the Rust form is an unparsed
    // slice, which is a shape `ToSchema` cannot derive. `value_type` names the
    // difference, which is the one thing an override is for.
    /// The object to seal. agentsfleet reads nothing inside it beyond its
    /// shape.
    ///
    /// Send any well-formed JSON object.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    #[serde(borrow)]
    pub data: &'a RawValue,
}

/// `PUT /v1/workspaces/{workspace_id}/secrets/{secret_name}` — replace the body.
//
// The same `data` object create takes, and no merge: a field omitted here is
// absent from the stored secret afterwards. Merging cannot express intent on a
// resource the caller can never read back — the `PATCH {api_key}` this
// replaced added an unused field to any secret not keyed `api_key`, left the
// live credential stale, and answered 200.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize)]
pub struct ReplaceSecretRequest<'a> {
    /// The complete replacement body.
    ///
    /// Opaque for the same reason create's is: an unparsed slice in Rust, an
    /// object on the wire. `value_type` names the difference.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    #[serde(borrow)]
    pub data: &'a RawValue,
}

/// What a create and a replace both answer with.
///
/// One shape for both verbs because both say the same thing — which name now
/// holds a secret — and the status code is what distinguishes them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredSecretResponse<'a> {
    /// The name the secret is stored under.
    pub name: Cow<'a, str>,
}

/// One credential as the list shows it.
//
// `model` is documented as optional in `SecretSummary` and is not emitted:
// `vault.secrets` has no column for it, and answering it would mean decrypting
// every row on a page that displays no secrets. See `afd_vault::projection`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretSummary<'a> {
    /// The name the secret is stored and interpolated under.
    pub name: Cow<'a, str>,
    /// When it was first stored, in epoch milliseconds.
    pub created_at: i64,
    /// The server's classification. A client reads this and never re-derives it.
    pub kind: &'static str,
    /// The provider label, for the kinds that carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<Cow<'a, str>>,
    /// The custom endpoint, where one may be displayed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<Cow<'a, str>>,
}

/// `GET /v1/workspaces/{workspace_id}/secrets` — every secret the workspace holds.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretsResponse<'a> {
    /// The rows, by name.
    pub secrets: Vec<SecretSummary<'a>>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{SecretSummary, SecretsResponse, StoreSecretRequest, StoredSecretResponse};
    use std::borrow::Cow;

    #[test]
    fn a_create_and_a_replace_answer_with_the_name_and_nothing_else() {
        // `{ name }` is the whole documented body for both verbs, and the
        // status code is what distinguishes them. A field added here would be a
        // shape change on two routes at once.
        let body = serde_json::to_string(&StoredSecretResponse {
            name: Cow::Borrowed("anthropic-prod"),
        })
        .expect("the response serializes");

        assert_eq!(body, r#"{"name":"anthropic-prod"}"#);
    }

    #[test]
    fn a_workspace_holding_nothing_answers_an_empty_array() {
        // Never `null`. A client iterating this list should not have to branch
        // on its absence first, and `hx.res.json(.{ .secrets = creds })` emits
        // `[]` for an empty slice too.
        let body = serde_json::to_string(&SecretsResponse { secrets: vec![] })
            .expect("the response serializes");

        assert_eq!(body, r#"{"secrets":[]}"#);
    }

    #[test]
    fn an_opaque_secret_carries_no_descriptor_keys_at_all() {
        // The dashboard's `Secret` union gives `custom_secret` no `provider`
        // and no `base_url`. Emitting them as `null` would be a field the union
        // says that variant does not have, which is why this one list departs
        // from the crate's null-stays-on-the-wire rule.
        let body = serde_json::to_string(&SecretsResponse {
            secrets: vec![SecretSummary {
                name: Cow::Borrowed("stripe"),
                created_at: 1_777_507_200_000,
                kind: "custom_secret",
                provider: None,
                base_url: None,
            }],
        })
        .expect("the response serializes");

        assert_eq!(
            body,
            r#"{"secrets":[{"name":"stripe","created_at":1777507200000,"kind":"custom_secret"}]}"#
        );
    }

    #[test]
    fn a_custom_endpoint_carries_both_descriptors() {
        let body = serde_json::to_string(&SecretSummary {
            name: Cow::Borrowed("gateway"),
            created_at: 1,
            kind: "custom_endpoint",
            provider: Some(Cow::Borrowed("openai-compatible")),
            base_url: Some(Cow::Borrowed("https://gw.example.com/v1")),
        })
        .expect("the row serializes");

        assert_eq!(
            body,
            r#"{"name":"gateway","created_at":1,"kind":"custom_endpoint","provider":"openai-compatible","base_url":"https://gw.example.com/v1"}"#
        );
    }

    #[test]
    fn a_create_body_keeps_its_data_verbatim_and_ignores_unknown_fields() {
        // `ignore_unknown_fields = true` on the Zig side, and the parity is the
        // absence of `deny_unknown_fields` here.
        let request: StoreSecretRequest<'_> = serde_json::from_str(
            r#"{"name":"openai","data":{"provider":"openai","api_key":"sk"},"extra":1}"#,
        )
        .expect("the request parses");

        assert_eq!(request.name, "openai");
        assert_eq!(
            request.data.get(),
            r#"{"provider":"openai","api_key":"sk"}"#
        );
    }
}
