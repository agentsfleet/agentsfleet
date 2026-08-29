//! What a verified payload says, and how each claim is read out of it.
//!
//! Split from the verifier for the reason [`crate::jwt`] is: the verifier owns
//! ORDER — signature before claims, and the configuration and clock the policy
//! needs — while this owns SHAPE. Reading a claim out of a JSON object is a
//! pure function over an already-verified payload, so every ladder below is
//! testable without a key, a clock or a network.
//!
//! # Absence is not one thing
//!
//! Two claims here are absent-tolerant and one is not, and the difference is
//! not style. See [`Claims::ceiling`].

use afd_auth::verifier::VerifyError;
use afd_core::id::Uuid7;

/// The tenant claim's name, in both places it may appear.
pub(crate) const CLAIM_TENANT_ID: &str = "tenant_id";
/// The workspace-ceiling claim's name.
const CLAIM_WORKSPACE_ID: &str = "workspace_id";
/// The object the provider nests its metadata claims under.
///
/// `clerk_metadata_payload.zig` writes exactly two keys into `public_metadata`,
/// and the session-token template projects `metadata.tenant_id` — so on a real
/// deployment the tenant is NESTED, and a reader that only looked at the top
/// level would find it on no production token at all.
const CLAIM_METADATA: &str = "metadata";

/// The claims this daemon reads. Everything else the issuer sends is ignored.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: Option<String>,
    pub(crate) iss: Option<String>,
    pub(crate) aud: Option<serde_json::Value>,
    pub(crate) exp: Option<i64>,
    /// The "not before" instant, checked when the issuer sends one.
    ///
    /// Named rather than left in `rest` so a token carrying two of them is a
    /// duplicate-field refusal rather than a silent last-one-wins.
    pub(crate) nbf: Option<i64>,
    /// Everything else, so the nested metadata object stays reachable without
    /// naming a second struct for one lookup.
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
    /// The capability claim, top level ONLY.
    ///
    /// `claims.zig` is emphatic about this: an earlier ladder tried `OAuth2`'s
    /// `scope` BEFORE this one, so a token carrying a standard `scope` claim
    /// would silently have supplied a different capability set. One place, and
    /// a reader that cannot say which value it trusted is the bug.
    pub(crate) scopes: Option<String>,
}

impl Claims {
    /// The raw string of `name`, read top-level first and then under `metadata`.
    ///
    /// The ladder `claims.zig::getClerkTenantId` walks, and in that order: a
    /// top-level projection wins over the nested one, so a template that starts
    /// projecting to the top level does not need both readers changed at once.
    /// One function rather than one per claim, so the two readers below cannot
    /// drift in WHERE they look.
    fn raw_claim(&self, name: &str) -> Option<&str> {
        self.rest
            .get(name)
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                self.rest
                    .get(CLAIM_METADATA)?
                    .as_object()?
                    .get(name)?
                    .as_str()
            })
    }

    /// An IDENTIFYING claim, where an unreadable value reads as absent.
    ///
    /// Deliberate, and only safe because of what absence costs here: the daemon
    /// refuses a principal with no tenant anyway, so both roads end in a
    /// refusal, and failing the whole verification would report a provisioning
    /// problem as a bad token.
    pub(crate) fn identifier(&self, name: &str) -> Option<Uuid7> {
        self.raw_claim(name).and_then(|raw| Uuid7::parse(raw).ok())
    }

    /// The workspace ceiling — absent when unset, an ERROR when unreadable.
    ///
    /// # Why this one is fallible when [`Self::identifier`] is not
    ///
    /// For an identifying claim, absent and unreadable mean the same thing and
    /// both end in a refusal. For a NARROWING one they are opposites. An absent
    /// ceiling means no ceiling — the permissive default every token in service
    /// relies on — while an unreadable one is a restriction an operator applied
    /// and this daemon cannot honour. Reading the second as the first grants
    /// exactly the access the ceiling existed to withhold, and reports nothing,
    /// because from the authoriser's side it is indistinguishable from a person
    /// who was never confined at all.
    ///
    /// The identifier is strict — canonical lowercase, version 7, RFC 4122
    /// variant — so this is not a theoretical arm: a v4 identifier, an
    /// uppercase one, or a prefixed one all land here.
    ///
    /// # Errors
    /// [`VerifyError::UnreadableCeiling`] when the claim is present and is not
    /// a canonical identifier.
    pub(crate) fn ceiling(&self) -> Result<Option<Uuid7>, VerifyError> {
        let Some(raw) = self.raw_claim(CLAIM_WORKSPACE_ID) else {
            return Ok(None);
        };
        Uuid7::parse(raw)
            .map(Some)
            .map_err(|_unreadable| VerifyError::UnreadableCeiling)
    }

    /// Whether `aud` names `wanted`, as a string or inside an array.
    ///
    /// Both shapes are legal in the specification and providers use both, so
    /// accepting only one would refuse a conforming token.
    pub(crate) fn audience_contains(&self, wanted: &str) -> bool {
        match &self.aud {
            Some(serde_json::Value::String(one)) => one == wanted,
            Some(serde_json::Value::Array(many)) => many
                .iter()
                .any(|item| item.as_str().is_some_and(|value| value == wanted)),
            _ => false,
        }
    }
}
