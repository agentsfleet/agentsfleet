//! The self-managed credential's shape, and the rule for its key.
//!
//! # Which half of the answer comes from where
//!
//! The CREDENTIAL supplies the provider, the key and the endpoint. The
//! registry ENTRY supplies the model and the context ceiling: a known-provider
//! credential is just `{provider, api_key}` with no `model` field at all, and
//! reading the model off the credential would resolve whatever a stale secret
//! happened to carry instead of what the tenant activated.
//!
//! The credential's own `model` is therefore parsed — a non-string is still
//! malformed — and then deliberately unread on the resolution path. Activation
//! reads it as the default it stores when the provider request names no model
//! (`agentsfleet tenant provider create --secret X` without `--model`).
//!
//! # The key is required, for every provider
//!
//! A custom OpenAI-compatible gateway included: activation refuses a
//! credential without a key, and the registry read that precedes this one
//! never returns it, so a run could not resolve a keyless body anyway. The rule
//! is one expression here, so it cannot come apart from the activation check.

use serde::Deserialize;

use crate::error::{Result, provider_malformed};
use af_crypto::secret::SecretString;

/// The credential field a self-managed resolution cannot proceed without.
pub(super) const FIELD_PROVIDER: &str = "provider";

/// The credential field a named provider cannot proceed without.
const FIELD_API_KEY: &str = "api_key";

/// A tenant's own provider credential, as the vault row holds it.
///
/// Unknown fields are ignored: this is a general-purpose vault row and a tenant
/// may address other fields of it as `${secrets.<name>.<field>}` at the tool
/// bridge. Refusing them would make one shared credential unusable for both.
#[derive(Debug, Deserialize)]
pub(super) struct Credential {
    /// Which provider this credential is for. Always required.
    pub(super) provider: Box<str>,
    /// The bearer key. Optional in the JSON, and required by the rule below.
    #[serde(default)]
    pub(super) api_key: Option<SecretString>,
    /// The credential's own model.
    ///
    /// Unread on the RESOLUTION path — the model lives on the tenant's
    /// registry entry, so reading it here would resolve whatever a stale
    /// secret happened to carry. Activation reads it as the default it stores
    /// when the provider request names no model.
    #[serde(default)]
    pub(super) model: Option<Box<str>>,
    /// A custom endpoint, validated against `provider` before anything is
    /// resolved.
    #[serde(default)]
    pub(super) base_url: Option<Box<str>>,
}

/// The key this credential resolves with.
///
/// A blank key and an absent key are the same fact, so they are one arm here
/// rather than two.
pub(super) fn bearer(api_key: Option<SecretString>) -> Result<SecretString> {
    match api_key {
        Some(key) if !key.is_empty() => Ok(key),
        _absent_or_blank => Err(provider_malformed(FIELD_API_KEY)),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::bearer;
    use crate::error::Result;
    use crate::provider::vetted::{Vetted, vet};
    use af_crypto::secret::SecretString;

    /// What resolution does with a stored body: vet it, then apply the key rule.
    fn resolve(body: &[u8]) -> Result<(Vetted, SecretString)> {
        let mut vetted = vet(body)?;
        let key = bearer(vetted.api_key.take())?;
        Ok((vetted, key))
    }

    #[test]
    fn the_credential_supplies_the_provider_and_the_key() {
        // The asymmetry the module note is about: a known-provider credential
        // carries NO model, and the model comes from the registry entry.
        let (vetted, key) = resolve(br#"{"provider":"openai","api_key":"sk-tenant"}"#)
            .expect("a named credential with a key resolves");

        assert_eq!(&*vetted.provider, "openai", "from the credential");
        assert_eq!(vetted.model, None, "the credential names no model");
        assert_eq!(key.expose(), "sk-tenant");
    }

    #[test]
    fn a_credentials_own_model_is_parsed_and_carried_for_activation() {
        // Parsed: a non-string `model` is still a malformed credential.
        assert!(resolve(br#"{"provider":"openai","api_key":"sk","model":42}"#).is_err());
        // Carried, not applied: activation reads it as a default when the
        // request names no model; resolution never does.
        let (vetted, _key) =
            resolve(br#"{"provider":"openai","api_key":"sk","model":"gpt-4o-mini"}"#)
                .expect("a credential carrying a model resolves");
        assert_eq!(vetted.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn every_provider_needs_a_key_a_compatible_gateway_included() {
        let (_vetted, key) = resolve(
            br#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1","api_key":"sk"}"#,
        )
        .expect("a gateway with a key resolves");
        assert_eq!(key.expose(), "sk");

        for keyless in [
            br#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1"}"#
                .as_slice(),
            br#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1","api_key":""}"#
                .as_slice(),
            br#"{"provider":"openai"}"#.as_slice(),
            br#"{"provider":"openai","api_key":""}"#.as_slice(),
        ] {
            let refused = resolve(keyless).err().expect("every provider needs a key");
            assert!(refused.is_config_permanent(), "{keyless:?}");
        }
    }

    #[test]
    fn an_ssrf_refusal_ends_the_event_rather_than_re_polling_forever() {
        // Permanent, deliberately: classified transient, the delivery would
        // re-poll at the poll interval indefinitely and no terminal row would
        // ever be written. A stored URL pointing at the metadata service does
        // not become safe by being retried.
        let refused =
            resolve(br#"{"provider":"openai-compatible","base_url":"https://169.254.169.254/v1"}"#)
                .err()
                .expect("the metadata service is not a gateway");

        assert!(
            refused.is_config_permanent(),
            "an SSRF-refused endpoint is a stored configuration a human must fix"
        );
    }

    #[test]
    fn a_named_provider_may_not_smuggle_an_endpoint() {
        let refused =
            resolve(br#"{"provider":"openai","api_key":"sk","base_url":"https://evil.test"}"#)
                .err()
                .expect("a named provider carries no endpoint");

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
            let refused = resolve(body)
                .err()
                .expect("a providerless credential cannot resolve");
            assert!(refused.is_config_permanent(), "{body:?}");
        }
    }
}
