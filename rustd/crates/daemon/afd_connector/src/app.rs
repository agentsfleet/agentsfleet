//! The platform app credentials one connector spends, from the deployment's
//! own vault.
//!
//! # One OAuth app per connector, serving every tenant
//!
//! A connector's `client_id`/`client_secret` pair belongs to whoever RUNS this
//! deployment, not to the workspace doing the connecting — so it is read from
//! the admin workspace under `<provider>-app` rather than from the tenant's.
//! The key is built in exactly one place (RULE UFS): [`Provider::app_key`].
//!
//! # Slack's signing secret lives in the same bag, and that is why this is one module
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

use std::collections::BTreeSet;

use af_core::error_code;
use af_core::id::Uuid7;
use af_crypto::secret::{SecretBytes, SecretString};
use afd_vault::Vault;
use afd_vault::bag::{Bag, BagFault, filled, secret, usable};
use serde::Deserialize;

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
        let key = provider.app_key();
        let read = self.vault.bag::<ConnectorApp>(Some(admin), &key).await?;
        Ok(unusable(read, &key).map(|found| found.0))
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
        let key = provider.app_key();
        let read = self.vault.bag::<SigningSecret>(Some(admin), &key).await?;
        Ok(unusable(read, &key).map(|found| found.0))
    }

    /// Which providers this deployment has configured an app for.
    ///
    /// One listing, no decryption: [`Provider::app_key`] derives the name, and
    /// membership answers the catalogue's `configured` column without opening a
    /// single envelope. The answer is about presence, and paying to decrypt
    /// five client secrets to render five booleans would put plaintext in the
    /// process for a page that displays none.
    ///
    /// A deployment with no admin workspace holds no apps, which is why the
    /// caller passes an `Option` and gets an empty set rather than a refusal:
    /// every provider then reads as not configured, which is exactly true.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn provisioned(&self, admin: Option<&Uuid7>) -> Result<BTreeSet<Provider>> {
        let Some(admin) = admin else {
            return Ok(BTreeSet::new());
        };
        let stored: BTreeSet<String> = self
            .vault
            .directory()
            .list(admin)
            .await?
            .into_iter()
            .map(|secret| secret.name)
            .collect();

        Ok(Provider::ALL
            .iter()
            .copied()
            .filter(|provider| stored.contains(&provider.app_key()))
            .collect())
    }
}

/// An app bag that exists and cannot be used leaves a line saying why; the
/// connector it belongs to is then unconfigured.
const EVENT_APP_UNUSABLE: &str = "connector_app_unusable";

/// A bag read down to its value, with a line for one that exists and fails its
/// schema.
fn unusable<B>(read: core::result::Result<B, BagFault>, key: &str) -> Option<B> {
    let error_code = error_code::CONNECTOR_NOT_CONFIGURED.as_str();
    usable(read, key, EVENT_APP_UNUSABLE, error_code)
}

/// The OAuth half of a `<provider>-app` bag as stored. Each reader decodes only
/// its own fields, so a malformed field of the other half cannot fail it.
#[derive(Deserialize)]
struct StoredOauth {
    client_id: Option<String>,
    client_secret: Option<SecretString>,
}

/// Slack's signing secret, the half of the bag the events ingress reads.
#[derive(Deserialize)]
struct StoredSigning {
    signing_secret: Option<SecretString>,
}

/// The OAuth pair a `<provider>-app` bag holds.
struct ConnectorApp(AppCredentials);

impl Bag for ConnectorApp {
    type Stored = StoredOauth;

    fn from_stored(stored: StoredOauth) -> core::result::Result<Self, BagFault> {
        Ok(Self(AppCredentials {
            client_id: filled(stored.client_id, FIELD_CLIENT_ID)?,
            client_secret: bytes(&secret(stored.client_secret, FIELD_CLIENT_SECRET)?),
        }))
    }
}

/// What a provider signs its deliveries to this deployment with.
struct SigningSecret(SecretBytes);

impl Bag for SigningSecret {
    type Stored = StoredSigning;

    fn from_stored(stored: StoredSigning) -> core::result::Result<Self, BagFault> {
        secret(stored.signing_secret, FIELD_SIGNING_SECRET).map(|found| Self(bytes(&found)))
    }
}

/// A secret field as the zeroing bytes the exchange and the signature wall take.
fn bytes(value: &SecretString) -> SecretBytes {
    SecretBytes::new(value.expose().as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use af_crypto::secret::SecretBytes;
    use afd_vault::bag::{BagFault, parse};

    use super::{ConnectorApp, SigningSecret};

    fn stored(json: &str) -> SecretBytes {
        SecretBytes::new(json.as_bytes().to_vec())
    }

    /// Every shape that names no usable value reads as absent.
    ///
    /// Each of these reaching the caller as "not configured" is what makes the
    /// refusal `UZ-CONN-001` rather than an exchange attempted with an empty
    /// client secret.
    #[test]
    fn no_unusable_shape_yields_a_field() {
        let unusable = [
            ("{}", BagFault::Missing("client_id")),
            (r#"{"other":"x"}"#, BagFault::Missing("client_id")),
            (r#"{"client_id":42}"#, BagFault::Malformed),
            (r#"{"client_id":null}"#, BagFault::Missing("client_id")),
            (r#"{"client_id":""}"#, BagFault::Missing("client_id")),
            (r#"{"client_id":{"nested":"x"}}"#, BagFault::Malformed),
            (r#"{"client_id":"cid"}"#, BagFault::Missing("client_secret")),
            (r#"["cid","csec"]"#, BagFault::Malformed),
        ];
        for (body, fault) in unusable {
            assert_eq!(
                parse::<ConnectorApp>(&stored(body)).err(),
                Some(fault),
                "`{body}` names no usable app"
            );
        }
    }

    /// The three fields are read out of one document, each on its own.
    ///
    /// The property that makes this one module: a bag carrying all three is
    /// read once per reader, and no field bleeds into another's value.
    #[test]
    fn the_three_fields_of_one_bag_are_read_apart() {
        let body = stored(r#"{"client_id":"cid","client_secret":"csec","signing_secret":"ssec"}"#);

        let app = parse::<ConnectorApp>(&body)
            .expect("a complete app parses")
            .0;
        assert_eq!(app.client_id, "cid");
        assert_eq!(app.client_secret.expose(), b"csec");
        let signing = parse::<SigningSecret>(&body).expect("the signing secret parses");
        assert_eq!(signing.0.expose(), b"ssec");
    }

    /// A malformed field of one half does not fail the other half's read.
    #[test]
    fn a_malformed_oauth_field_does_not_break_the_signing_read() {
        let body = stored(r#"{"client_id":42,"signing_secret":"ssec"}"#);

        assert_eq!(
            parse::<ConnectorApp>(&body).err(),
            Some(BagFault::Malformed)
        );
        let signing = parse::<SigningSecret>(&body).expect("the signing half is well formed");
        assert_eq!(signing.0.expose(), b"ssec");
    }

    /// A bag with the OAuth pair and no signing secret is still an OAuth app.
    ///
    /// Four of the five connectors sign nothing back to this deployment, so a
    /// reader that required all three would refuse every one of them.
    #[test]
    fn a_bag_without_a_signing_secret_still_carries_its_oauth_pair() {
        let body = stored(r#"{"client_id":"cid","client_secret":"csec"}"#);

        assert_eq!(parse::<ConnectorApp>(&body).err(), None);
        assert_eq!(
            parse::<SigningSecret>(&body).err(),
            Some(BagFault::Missing("signing_secret"))
        );
    }
}
