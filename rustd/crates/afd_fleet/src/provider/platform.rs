//! The platform strategy: the key is ours, and the default is read live.
//!
//! Everything about the resolution comes from the active
//! `core.platform_provider_defaults` row — provider, model, context ceiling and
//! endpoint — and NOT from the tenant's own `tenant_model_selection` snapshot,
//! even when that snapshot exists and says `platform`. That is what makes an
//! admin repointing the default in the dashboard take effect on the next lease
//! for every platform-mode tenant, with no redeploy and no per-tenant write.
//!
//! The vault key name is the PROVIDER: `resolvePlatformDefault` passes
//! `plk.provider` as the key it loads from the admin workspace, so one
//! provider's platform credential is one row.

use serde::Deserialize;

use crate::error::{Result, provider_malformed, provider_platform_key_missing};
use crate::money::Posture;
use crate::provider::endpoint;
use crate::provider::resolved::{Dialled, Resolved, SecretString};
use crate::provider::selection::PlatformDefault;
use crate::provider::{Resolution, Strategy};
use crate::vault::KeyRef;

/// The credential field a platform default cannot resolve without.
const FIELD_API_KEY: &str = "api_key";

/// The platform's own credential, as the vault row holds it.
///
/// One field, and every other key in the object is ignored rather than refused:
/// a platform credential is an ordinary vault row and an operator may have put
/// a label or a rotation note beside the key. `deny_unknown_fields` would turn
/// that into an outage for every platform-mode tenant at once.
#[derive(Debug, Deserialize)]
struct Credential {
    /// The provider key. Required, and required to be non-empty — a platform
    /// default with a blank key is a configuration an operator must fix, not a
    /// keyless dial.
    api_key: SecretString,
}

/// Resolution through the active platform default.
#[derive(Debug)]
pub(super) struct Platform(PlatformDefault);

impl Platform {
    /// The strategy for the one active default, or nothing configured.
    ///
    /// # Errors
    /// Reports an absent active row. An operator-side incident rather than a
    /// tenant one, and permanent: no lease can be priced until someone sets a
    /// default, and waiting does not set one.
    pub(super) fn prepare(default: Option<PlatformDefault>) -> Result<Strategy> {
        default
            .map(|row| Box::new(Self(row)) as Strategy)
            .ok_or_else(provider_platform_key_missing)
    }
}

impl Resolution for Platform {
    fn key(&self) -> KeyRef<'_> {
        KeyRef {
            workspace_id: &self.0.source_workspace_id,
            name: &self.0.provider,
        }
    }

    /// The row supplies everything but the key; the credential supplies only
    /// the key.
    ///
    /// The endpoint is carried forward from the ROW rather than validated here,
    /// and that asymmetry with [`super::managed`] is deliberate:
    /// `PUT /v1/admin/platform-keys` runs the same guard at write time, so the
    /// stored value has already passed it. Re-validating would be a defensive
    /// re-check of a value the type system got from a checked writer — which
    /// the functional rule says to delete, not to add.
    fn interpret(&self, body: &[u8]) -> Result<Resolved> {
        let credential: Credential = super::credential(body, FIELD_API_KEY)?;
        if credential.api_key.is_empty() {
            return Err(provider_malformed(FIELD_API_KEY));
        }

        Ok(Resolved::new(
            Posture::Platform,
            self.0.provider.clone(),
            self.0.model.clone(),
            self.0.context_cap_tokens,
            dialled(self.0.base_url.as_deref()),
            credential.api_key,
        ))
    }
}

/// The platform default's endpoint, when it has one this daemon will admit.
///
/// # The asymmetry here is deliberate, and it is worth naming
///
/// A TENANT's endpoint is refused outright when it fails
/// [`endpoint::validate`] — `resolve` returns the rejection and the whole
/// resolution fails. The platform default is an OPERATOR's row, so refusing it
/// would take down every fleet on the platform default rather than one tenant's
/// fleet. It degrades instead: an endpoint that will not validate yields
/// `None`, the lease carries no `inference_host`, and the egress allowlist
/// therefore admits nothing for it. Fail-closed at the run rather than
/// fail-closed at boot.
///
/// The Zig applies no check on this path at all — `hostFromUrl` takes whatever
/// the column held. Validating here is stricter, and free.
fn dialled(base_url: Option<&str>) -> Option<Dialled> {
    let url = base_url?;
    let host = endpoint::validate(url).ok()?;
    Some(Dialled {
        base_url: url.into(),
        inference_host: host,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Credential, Platform};
    use crate::money::Posture;
    use crate::provider::selection::PlatformDefault;
    use afd_core::id::Uuid7;

    fn workspace() -> Uuid7 {
        Uuid7::parse("0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0b").expect("a canonical v7 identifier")
    }

    fn default_row() -> PlatformDefault {
        PlatformDefault {
            provider: "anthropic".into(),
            source_workspace_id: workspace(),
            model: "claude-opus-5".into(),
            base_url: None,
            context_cap_tokens: 200_000,
        }
    }

    #[test]
    fn the_key_is_read_from_the_admin_workspace_under_the_provider_name() {
        let strategy = Platform::prepare(Some(default_row())).expect("an active row resolves");
        let key = strategy.key();

        assert_eq!(key.workspace_id, &workspace());
        assert_eq!(
            key.name, "anthropic",
            "the vault key name IS the provider, per resolvePlatformDefault"
        );
    }

    #[test]
    fn no_active_default_is_a_permanent_refusal() {
        let refused = Platform::prepare(None).expect_err("nothing configured cannot resolve");

        assert!(
            refused.is_config_permanent(),
            "an unset platform default does not fix itself by being retried"
        );
    }

    #[test]
    fn the_row_supplies_everything_except_the_key() {
        let strategy = Platform::prepare(Some(default_row())).expect("an active row resolves");
        let resolved = strategy
            .interpret(br#"{"api_key":"sk-platform","note":"rotated 2026-08"}"#)
            .expect("a credential carrying a key resolves");

        assert_eq!(resolved.posture, Posture::Platform);
        assert_eq!(&*resolved.provider, "anthropic");
        assert_eq!(&*resolved.model, "claude-opus-5");
        assert_eq!(resolved.context_cap_tokens, 200_000);
        assert_eq!(resolved.endpoint, None);
        assert_eq!(resolved.api_key().expose(), "sk-platform");
    }

    #[test]
    fn an_admin_configured_endpoint_travels_to_the_dial() {
        // A non-named default has to actually dial the endpoint the operator
        // set, or it silently falls back to a built-in host.
        let row = PlatformDefault {
            provider: "openai-compatible".into(),
            base_url: Some("https://gw.example.com/v1".into()),
            ..default_row()
        };
        let strategy = Platform::prepare(Some(row)).expect("an active row resolves");
        let resolved = strategy
            .interpret(br#"{"api_key":"sk-gateway"}"#)
            .expect("a credential carrying a key resolves");

        assert_eq!(
            resolved.endpoint.as_ref().map(|e| e.base_url.as_ref()),
            Some("https://gw.example.com/v1")
        );
    }

    #[test]
    fn a_platform_credential_without_a_usable_key_is_malformed() {
        let strategy = Platform::prepare(Some(default_row())).expect("an active row resolves");
        for body in [
            br"{}".as_slice(),
            br#"{"api_key":""}"#.as_slice(),
            br#"{"api_key":42}"#.as_slice(),
            br#"["not","an","object"]"#.as_slice(),
            b"not json at all".as_slice(),
        ] {
            let refused = strategy
                .interpret(body)
                .expect_err("a keyless platform credential cannot resolve");
            assert!(refused.is_config_permanent(), "{body:?}");
        }
    }

    #[test]
    fn a_credential_never_renders_its_key() {
        let credential: Credential =
            serde_json::from_slice(br#"{"api_key":"sk-platform"}"#).expect("a keyed credential");
        let rendered = format!("{credential:?}");

        assert!(!rendered.contains("sk-platform"), "{rendered}");
    }
}
