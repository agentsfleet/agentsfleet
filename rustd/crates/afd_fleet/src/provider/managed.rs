//! The self-managed strategy: the tenant's key, in the tenant's own workspace.
//!
//! # Which half of the answer comes from where
//!
//! The CREDENTIAL supplies the provider, the key and the endpoint. The
//! SELECTION row supplies the model and the context ceiling. That split is not
//! arbitrary and it is not symmetric with [`super::platform`]: the model lives
//! on the tenant's registry entry (M121), so a known-provider credential is
//! just `{provider, api_key}` with no `model` field at all, and reading the
//! model off the credential would resolve whatever a stale secret happened to
//! carry instead of what the tenant activated.
//!
//! The credential's own `model` is therefore parsed — a non-string is still
//! malformed — and then deliberately unread on this path. It survives only as
//! the legacy fallback for a bare tenant-plane `PUT /provider` that passes no
//! model, which is M178's surface and not this one's.
//!
//! # The key is optional, for exactly one provider
//!
//! A custom OpenAI-compatible gateway may be keyless — that is the spec's
//! optional-key design, and a tenant fronting their own vLLM has no bearer
//! token to give. Every NAMED provider must carry a non-empty key. The two
//! rules are one expression here rather than the Zig's `is_compatible` flag
//! computed once and consulted twice.

use serde::Deserialize;

use crate::error::{Result, provider_endpoint, provider_malformed};
use crate::money::Posture;
use crate::provider::endpoint;
use crate::provider::resolved::{Resolved, SecretString};
use crate::provider::selection::Selection;
use crate::provider::{Resolution, Strategy};
use crate::vault::KeyRef;
use afd_core::id::Uuid7;

/// The credential field a self-managed resolution cannot proceed without.
const FIELD_PROVIDER: &str = "provider";

/// The credential field a named provider cannot proceed without.
const FIELD_API_KEY: &str = "api_key";

/// The selection column that says WHICH vault row to open.
const FIELD_SECRET_REF: &str = "secret_ref";

/// A tenant's own provider credential, as the vault row holds it.
///
/// Unknown fields are ignored: this is a general-purpose vault row and a tenant
/// may address other fields of it as `${secrets.<name>.<field>}` at the tool
/// bridge. Refusing them would make one shared credential unusable for both.
#[derive(Debug, Deserialize)]
struct Credential {
    /// Which provider this credential is for. Always required.
    provider: Box<str>,
    /// The bearer key. Optional in the JSON, and required by the rule below
    /// for every provider but the compatible one.
    #[serde(default)]
    api_key: Option<SecretString>,
    /// The credential's own model — parsed so a non-string is caught, and
    /// unread on this path. See the module note.
    #[expect(
        dead_code,
        reason = "parsed for its shape, not its value: a non-string `model` is a malformed                   credential, while a valid one is M178's legacy fallback and not this path's"
    )]
    #[serde(default)]
    model: Option<Box<str>>,
    /// A custom endpoint, validated against `provider` before anything is
    /// resolved.
    #[serde(default)]
    base_url: Option<Box<str>>,
}

/// Resolution through a tenant's own stored credential.
#[derive(Debug)]
pub(super) struct SelfManaged {
    /// What the tenant activated: the model and the ceiling.
    selection: Selection,
    /// The vault row's name, lifted out of the selection's nullable column.
    secret_ref: Box<str>,
    /// The workspace holding it.
    workspace_id: Uuid7,
}

impl SelfManaged {
    /// The strategy for a self-managed selection in `workspace_id`.
    ///
    /// The nullable `secret_ref` column is narrowed HERE, once, so nothing
    /// downstream carries an `Option` it would have to justify unwrapping: a
    /// `SelfManaged` that exists names a row. That is parse-don't-validate
    /// applied to a column the schema lets be null for a posture that cannot
    /// use it.
    ///
    /// # Errors
    /// Reports a self-managed row whose `secret_ref` is null — a selection
    /// pointing at no credential, which is permanent.
    pub(super) fn prepare(selection: Selection, workspace_id: Uuid7) -> Result<Strategy> {
        let secret_ref = selection
            .secret_ref
            .clone()
            .ok_or_else(|| provider_malformed(FIELD_SECRET_REF))?;
        Ok(Box::new(Self {
            selection,
            secret_ref,
            workspace_id,
        }))
    }
}

impl Resolution for SelfManaged {
    fn key(&self) -> KeyRef<'_> {
        KeyRef {
            workspace_id: &self.workspace_id,
            name: &self.secret_ref,
        }
    }

    fn interpret(&self, body: &[u8]) -> Result<Resolved> {
        let credential: Credential = super::credential(body, FIELD_PROVIDER)?;
        if credential.provider.is_empty() {
            return Err(provider_malformed(FIELD_PROVIDER));
        }

        // Endpoint first, BEFORE the key is looked at: a hostile or mismatched
        // endpoint fails the resolution while the credential is still just
        // bytes, which is the ordering `probeSelfManagedSecret` chose and the
        // reason it gave — nothing owned is built around a URL that will be
        // refused.
        let base_url: Option<Box<str>> =
            endpoint::resolve(&credential.provider, credential.base_url.as_deref())
                .map_err(|rejection| provider_endpoint(rejection.as_str()))?
                .map(Box::from);

        // A resolved endpoint IS the compatible provider — `endpoint::resolve`
        // has already refused every other pairing — so the optional-key rule
        // reads off the outcome rather than re-comparing the provider string.
        // The Zig computes an `is_compatible` flag and consults it twice, which
        // is two places for the two rules to come apart.
        let api_key = bearer(credential.api_key, base_url.is_some())?;

        Ok(Resolved::new(
            Posture::SelfManaged,
            credential.provider,
            self.selection.model.clone(),
            self.selection.context_cap_tokens,
            base_url,
            api_key,
        ))
    }
}

/// The key this credential resolves with, given whether one is optional.
///
/// A blank key and an absent key are the same fact — the Zig folds both to `""`
/// and then length-checks — so they are one arm here rather than two.
fn bearer(api_key: Option<SecretString>, keyless_permitted: bool) -> Result<SecretString> {
    match api_key {
        Some(key) if !key.is_empty() => Ok(key),
        _absent_or_blank if keyless_permitted => Ok(SecretString::new(String::new())),
        _named_provider => Err(provider_malformed(FIELD_API_KEY)),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::SelfManaged;
    use crate::money::Posture;
    use crate::provider::selection::Selection;
    use afd_core::id::Uuid7;

    fn workspace() -> Uuid7 {
        Uuid7::parse("0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0b").expect("a canonical v7 identifier")
    }

    fn selection() -> Selection {
        Selection {
            posture: Posture::SelfManaged,
            provider: "anthropic".into(),
            model: "claude-opus-5".into(),
            context_cap_tokens: 200_000,
            secret_ref: Some("my-anthropic-key".into()),
        }
    }

    fn strategy() -> crate::provider::Strategy {
        SelfManaged::prepare(selection(), workspace()).expect("a selection naming a row")
    }

    #[test]
    fn the_key_is_read_from_the_tenants_workspace_under_the_stored_name() {
        let strategy = strategy();
        let key = strategy.key();

        assert_eq!(key.workspace_id, &workspace());
        assert_eq!(key.name, "my-anthropic-key");
    }

    #[test]
    fn a_selection_naming_no_credential_is_a_permanent_refusal() {
        let orphan = Selection {
            secret_ref: None,
            ..selection()
        };
        let refused = SelfManaged::prepare(orphan, workspace())
            .expect_err("a null secret_ref cannot resolve");

        assert!(refused.is_config_permanent());
    }

    #[test]
    fn the_credential_supplies_the_provider_and_the_selection_supplies_the_model() {
        // The asymmetry the module note is about: a known-provider credential
        // carries NO model, and reading one off it would resolve whatever a
        // stale secret happened to hold instead of what the tenant activated.
        let resolved = strategy()
            .interpret(br#"{"provider":"openai","api_key":"sk-tenant"}"#)
            .expect("a named credential with a key resolves");

        assert_eq!(resolved.posture, Posture::SelfManaged);
        assert_eq!(&*resolved.provider, "openai", "from the credential");
        assert_eq!(&*resolved.model, "claude-opus-5", "from the selection");
        assert_eq!(resolved.context_cap_tokens, 200_000);
        assert_eq!(resolved.api_key().expose(), "sk-tenant");
        assert_eq!(resolved.base_url, None);
    }

    #[test]
    fn a_credentials_own_model_is_parsed_and_then_ignored() {
        // Parsed: a non-string `model` is still a malformed credential.
        strategy()
            .interpret(br#"{"provider":"openai","api_key":"sk","model":42}"#)
            .unwrap_err();
        // Ignored: a legacy credential carrying one does not override the
        // tenant's activated model.
        let resolved = strategy()
            .interpret(br#"{"provider":"openai","api_key":"sk","model":"gpt-4o-mini"}"#)
            .expect("a legacy credential still resolves");
        assert_eq!(&*resolved.model, "claude-opus-5");
    }

    #[test]
    fn a_compatible_gateway_may_be_keyless_and_a_named_provider_may_not() {
        let keyless = strategy()
            .interpret(
                br#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1"}"#,
            )
            .expect("a keyless gateway is the optional-key design");
        assert!(keyless.api_key().expose().is_empty());
        assert_eq!(
            keyless.base_url.as_deref(),
            Some("https://gw.example.com/v1")
        );

        for keyless_named in [
            br#"{"provider":"openai"}"#.as_slice(),
            br#"{"provider":"openai","api_key":""}"#.as_slice(),
        ] {
            let refused = strategy()
                .interpret(keyless_named)
                .expect_err("a named provider needs a key");
            assert!(refused.is_config_permanent(), "{keyless_named:?}");
        }
    }

    #[test]
    fn an_ssrf_refusal_ends_the_event_rather_than_re_polling_forever() {
        // A deliberate divergence: the Zig classifies this transient, so the
        // delivery re-polls at the poll interval indefinitely and no terminal
        // row is ever written. A stored URL pointing at the metadata service
        // does not become safe by being retried.
        let refused = strategy()
            .interpret(
                br#"{"provider":"openai-compatible","base_url":"https://169.254.169.254/v1"}"#,
            )
            .expect_err("the metadata service is not a gateway");

        assert!(
            refused.is_config_permanent(),
            "an SSRF-refused endpoint is a stored configuration a human must fix"
        );
    }

    #[test]
    fn a_named_provider_may_not_smuggle_an_endpoint() {
        let refused = strategy()
            .interpret(br#"{"provider":"openai","api_key":"sk","base_url":"https://evil.test"}"#)
            .expect_err("a named provider carries no endpoint");

        assert!(
            refused.is_config_permanent(),
            "a smuggled endpoint is the same stored-configuration fault"
        );
    }

    #[test]
    fn a_credential_with_no_usable_provider_is_malformed() {
        for body in [
            br"{}".as_slice(),
            br#"{"provider":""}"#.as_slice(),
            br#"{"provider":42,"api_key":"sk"}"#.as_slice(),
            br#"["not","an","object"]"#.as_slice(),
        ] {
            let refused = strategy()
                .interpret(body)
                .expect_err("a providerless credential cannot resolve");
            assert!(refused.is_config_permanent(), "{body:?}");
        }
    }
}
