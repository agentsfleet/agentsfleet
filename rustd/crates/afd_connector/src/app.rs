//! The platform app credentials one connector spends, from the deployment's
//! own vault.
//!
//! # One OAuth app per connector, serving every tenant
//!
//! A connector's `client_id`/`client_secret` pair belongs to whoever RUNS this
//! deployment, not to the workspace doing the connecting — so it is read from
//! the admin workspace under `<provider>-app` rather than from the tenant's.
//! `oauth2.zig`'s `loadAppCreds` reads the same bag from the same key, and the
//! key is built in exactly one place on either daemon (RULE UFS) —
//! [`Provider::app_key`] here, `APP_VAULT_KEY_SUFFIX` there.
//!
//! # Slack's signing secret lives in the same bag, and that is why this is one
//! module
//!
//! The Slack events ingress verifies a request signature against a secret that
//! is a FIELD of this same document — so the read is one read with two callers,
//! and splitting them would be two vault opens for one row and two places for
//! the key name to drift.
//!
//! # Every unusable shape is `Ok(None)`
//!
//! No bag, a body that is not an object, an absent field, a field that is not a
//! string, an empty string: all of them are "this deployment has not configured
//! this connector", which is `UZ-CONN-001` one layer up and never a connect
//! attempted with an empty secret. `afd_ingress::secret` fails closed the same
//! way and for the same reason.

use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_vault::{SecretName, Vault};

use crate::error::Result;
use crate::exchange::AppCredentials;
use crate::provider::Provider;

/// The bag's field holding the public half of the OAuth app.
const FIELD_CLIENT_ID: &str = "client_id";

/// The bag's field holding the half that proves this deployment owns the app.
const FIELD_CLIENT_SECRET: &str = "client_secret";

/// The bag's field holding what a provider signs its deliveries with.
///
/// Slack's today. Stored beside the OAuth pair rather than under a key of its
/// own because it is the same app: one registration at the vendor produces all
/// three, and an operator vaulting them separately is an operator who can
/// rotate half of an app.
const FIELD_SIGNING_SECRET: &str = "signing_secret";

/// This deployment's own connector app credentials.
///
/// Holds the KEY-bearing half of the vault: an app secret has to be opened to
/// be spent at the vendor, which is the same exception
/// [`afd_vault::Vault::load`] documents for the signature path.
///
/// Cheap to clone — [`Vault`] holds its key behind an `Arc`.
#[derive(Debug, Clone)]
pub struct PlatformApp {
    /// Where the `<provider>-app` bags are sealed.
    vault: Vault,
}

impl PlatformApp {
    /// Binds the reader to an already-opened vault.
    #[must_use]
    pub const fn new(vault: Vault) -> Self {
        Self { vault }
    }

    /// The OAuth app credentials this deployment connects `provider` with.
    ///
    /// `Ok(None)` when this deployment has configured no app for it — see the
    /// module note on why every unusable shape is one answer.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open. Neither is the person's fault and neither is told to them.
    pub async fn credentials(
        &self,
        admin: &Uuid7,
        provider: Provider,
    ) -> Result<Option<AppCredentials>> {
        let Some(bag) = self.bag(admin, provider).await? else {
            return Ok(None);
        };
        let (Some(client_id), Some(client_secret)) = (
            field(&bag, FIELD_CLIENT_ID),
            field(&bag, FIELD_CLIENT_SECRET),
        ) else {
            return Ok(None);
        };
        Ok(Some(AppCredentials {
            client_id,
            // Re-wrapped rather than handed out as a slice of the parsed tree:
            // `serde_json` owns that tree on the heap with no destructor that
            // zeroes it, so the copy this carries is the one with the guarantee.
            client_secret: SecretBytes::new(client_secret.into_bytes()),
        }))
    }

    /// What `provider` signs its deliveries to this deployment with.
    ///
    /// # Errors
    /// As [`Self::credentials`].
    pub async fn signing_secret(
        &self,
        admin: &Uuid7,
        provider: Provider,
    ) -> Result<Option<SecretBytes>> {
        let Some(bag) = self.bag(admin, provider).await? else {
            return Ok(None);
        };
        Ok(field(&bag, FIELD_SIGNING_SECRET).map(|secret| SecretBytes::new(secret.into_bytes())))
    }

    /// The `<provider>-app` document, parsed, or nothing.
    ///
    /// A name the vault would refuse resolves to `None` rather than raising:
    /// it is derived from a shipped provider id, so a refusal would be this
    /// build's bug and not an incident to wake an operator with mid-connect.
    async fn bag(&self, admin: &Uuid7, provider: Provider) -> Result<Option<serde_json::Value>> {
        let Ok(name) = SecretName::parse(&provider.app_key()) else {
            return Ok(None);
        };
        let Some(stored) = self.vault.load(admin, &name).await? else {
            return Ok(None);
        };
        Ok(serde_json::from_slice(stored.expose()).ok())
    }
}

/// One non-empty string field of the bag.
fn field(bag: &serde_json::Value, name: &str) -> Option<String> {
    let value = bag.get(name)?.as_str()?;
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{FIELD_CLIENT_ID, FIELD_CLIENT_SECRET, FIELD_SIGNING_SECRET, field};

    /// Every shape that names no usable value reads as absent.
    ///
    /// The set `afd_ingress::secret` proves for its own field, and it matters
    /// for the same reason: each of these reaching the caller as "not
    /// configured" is what makes the refusal `UZ-CONN-001` rather than an
    /// exchange attempted with an empty client secret.
    #[test]
    fn no_unusable_shape_yields_a_field() {
        let unusable = [
            "{}",
            r#"{"other":"x"}"#,
            r#"{"client_id":42}"#,
            r#"{"client_id":null}"#,
            r#"{"client_id":""}"#,
            r#"{"client_id":{"nested":"x"}}"#,
        ];
        for stored in unusable {
            let bag: serde_json::Value = serde_json::from_str(stored).expect("valid JSON fixture");
            assert!(
                field(&bag, FIELD_CLIENT_ID).is_none(),
                "`{stored}` names no usable client id",
            );
        }
    }

    /// The three fields are read out of one document, each on its own.
    ///
    /// The property that makes this one module: a bag carrying all three is
    /// read once, and no field bleeds into another's value.
    #[test]
    fn the_three_fields_of_one_bag_are_read_apart() {
        let bag: serde_json::Value = serde_json::from_str(
            r#"{"client_id":"cid","client_secret":"csec","signing_secret":"ssec"}"#,
        )
        .expect("valid JSON fixture");

        assert_eq!(field(&bag, FIELD_CLIENT_ID).as_deref(), Some("cid"));
        assert_eq!(field(&bag, FIELD_CLIENT_SECRET).as_deref(), Some("csec"));
        assert_eq!(field(&bag, FIELD_SIGNING_SECRET).as_deref(), Some("ssec"));
    }

    /// A bag with the OAuth pair and no signing secret is still an OAuth app.
    ///
    /// Four of the five connectors sign nothing back to this deployment, so a
    /// reader that required all three would refuse every one of them.
    #[test]
    fn a_bag_without_a_signing_secret_still_carries_its_oauth_pair() {
        let bag: serde_json::Value =
            serde_json::from_str(r#"{"client_id":"cid","client_secret":"csec"}"#)
                .expect("valid JSON fixture");

        assert!(field(&bag, FIELD_CLIENT_ID).is_some());
        assert!(field(&bag, FIELD_SIGNING_SECRET).is_none());
    }
}
