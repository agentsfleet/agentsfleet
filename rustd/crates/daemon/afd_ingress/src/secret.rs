//! Opening the one secret a signature is checked against.
//!
//! Every recipe reads the `webhook_secret` string from the credential object.
//! A Svix declaration puts its `whsec_` string in that same field.
//!
//! # Every failure here is `Ok(None)`, and that is the fail-closed answer
//!
//! A missing credential, a body that is not an object, an absent field, a field
//! that is not a string, an empty string: all of them resolve to no secret, and
//! no secret is [`af_webhook::Refusal::Unconfigured`] one layer up — never a
//! pass, and never a comparison against an empty key. `Scheme::verify_at`
//! carries the same guard again on its own side, which is defence in depth
//! rather than duplication: an empty key makes the tag deterministic and
//! attacker-computable, so two independent refusals is the right number.
//!
//! Nothing here logs the secret, its length, or any prefix of it. A length is a
//! narrowing fact about a key (LOG-SECRET, `.orly/standards/domains/logging.md`), and
//! [`afd_vault::Vault::load`] declines to log one for the same reason.

use af_core::error_code;
use af_core::id::Uuid7;
use af_crypto::secret::{SecretBytes, SecretString};
use afd_vault::SecretName;
use afd_vault::bag::{Bag, BagFault, secret, usable};
use serde::Deserialize;

use crate::Ingress;
use crate::binding::Binding;
use crate::error::Result;

/// The field of a stored credential object that holds the shared secret.
///
/// Pinned byte-for-byte: operators have already stored credentials under this
/// field, so the name is stored data rather than a spelling this crate may
/// improve.
const WEBHOOK_SECRET_FIELD: &str = "webhook_secret";

impl Ingress {
    /// The shared secret this fleet's provider signs with.
    ///
    /// `Ok(None)` for every way a fleet can turn out to have no usable secret —
    /// see the module note on why that is one answer rather than five.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open. Neither is the sender's fault, and neither is told to them.
    pub async fn signing_secret(&self, binding: &Binding) -> Result<Option<SecretBytes>> {
        self.webhook_secret(binding.workspace(), binding.credential_name())
            .await
    }

    /// The App's own signing secret, held by the platform admin workspace.
    ///
    /// An App delivery is signed with ONE secret for the whole installation —
    /// the App's, configured once by whoever runs this deployment — where a
    /// per-fleet delivery is signed with a secret the fleet's own workspace
    /// stores. So this reads a different workspace and takes the key by name
    /// rather than from a binding: there is no binding yet, and there cannot
    /// be, because the delivery has to be verified BEFORE it can be routed to
    /// the fleets that will run it.
    ///
    /// The stored SHAPE is the same JSON object with the same `webhook_secret`
    /// field, which is why this shares [`WebhookSecret`] rather than carrying a
    /// second reader.
    ///
    /// `Ok(None)` for every way the secret can turn out to be unusable, and for
    /// a deployment that has configured no admin workspace at all. All of them
    /// are [`af_webhook::Refusal::Unconfigured`] one layer up — never a pass.
    ///
    /// # Errors
    /// As [`Self::signing_secret`].
    pub async fn platform_secret(
        &self,
        admin_workspace: &Uuid7,
        key: &str,
    ) -> Result<Option<SecretBytes>> {
        self.webhook_secret(admin_workspace, key).await
    }

    /// A workspace credential's `webhook_secret`, by key name, or nothing.
    ///
    /// Every fault — no row, a name the vault would refuse, a body that is not
    /// an object, an absent or empty field — is `None`: it came from a fleet's
    /// stored document or this deployment's setup, so it is a misconfiguration
    /// to fail closed on, not an incident to wake an operator with at delivery
    /// time.
    async fn webhook_secret(&self, workspace: &Uuid7, key: &str) -> Result<Option<SecretBytes>> {
        // The name came from a fleet's own document, which may spell one longer
        // than the vault holds: no secret, and no build-fault line for it.
        if SecretName::parse(key).is_err() {
            return Ok(None);
        }
        let read = self
            .vault
            .bag::<WebhookSecret>(Some(workspace), key)
            .await?;
        let error_code = error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED.as_str();
        Ok(usable(read, key, EVENT_SECRET_UNUSABLE, error_code).map(|found| found.0))
    }
}

/// A credential that exists and holds no usable secret leaves a line saying
/// why; the delivery is then refused as unconfigured.
const EVENT_SECRET_UNUSABLE: &str = "webhook_secret_unusable";

/// A stored credential as the signature wall reads it: one secret field, and
/// every other field skipped by serde without being copied anywhere.
#[derive(Deserialize)]
struct StoredWebhook {
    webhook_secret: Option<SecretString>,
}

/// The `webhook_secret` field of a stored credential object, as key material.
///
/// The parsed string and the returned bytes both zero on drop.
struct WebhookSecret(SecretBytes);

impl Bag for WebhookSecret {
    type Stored = StoredWebhook;

    fn from_stored(stored: StoredWebhook) -> core::result::Result<Self, BagFault> {
        secret(stored.webhook_secret, WEBHOOK_SECRET_FIELD)
            .map(|field| Self(SecretBytes::new(field.expose().as_bytes().to_vec())))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use std::sync::Arc;

    use af_core::id::Uuid7;
    use af_crypto::entropy::Entropy;
    use af_crypto::secret::Kek;
    use afd_vault::bag::BagFault;

    use super::{SecretBytes, WebhookSecret};
    use crate::Ingress;

    /// Every shape a stored credential can take that names no usable secret.
    ///
    /// Each one of these reaching the wall as "no secret configured" is what
    /// makes the refusal `UZ-WH-020` rather than a tag computed against
    /// nothing.
    #[test]
    fn no_stored_shape_but_a_populated_field_yields_key_material() {
        let missing = BagFault::Missing("webhook_secret");
        let refused = [
            ("not json at all", BagFault::Malformed),
            ("[\"webhook_secret\"]", BagFault::Malformed),
            ("\"webhook_secret\"", BagFault::Malformed),
            ("{}", missing),
            ("{\"other\":\"x\"}", missing),
            ("{\"webhook_secret\":42}", BagFault::Malformed),
            ("{\"webhook_secret\":null}", missing),
            ("{\"webhook_secret\":\"\"}", missing),
        ];
        for (stored, fault) in refused {
            let body = SecretBytes::new(stored.as_bytes().to_vec());
            assert_eq!(
                afd_vault::bag::parse::<WebhookSecret>(&body).err(),
                Some(fault),
                "`{stored}` names no usable secret and must fail closed"
            );
        }
    }

    #[test]
    fn the_field_comes_back_as_its_own_zeroing_copy() {
        let body = SecretBytes::new(br#"{"webhook_secret":"s3cr3t","other":"x"}"#.to_vec());

        let secret = afd_vault::bag::parse::<WebhookSecret>(&body)
            .expect("the field is a non-empty string")
            .0;

        assert_eq!(secret.expose(), b"s3cr3t");
    }

    /// The sibling fields never leak into the key.
    ///
    /// A reader that handed back the whole document would still read as
    /// configured, and every signature would then fail to compare for a reason
    /// no log line would name.
    #[test]
    fn the_surrounding_document_is_not_the_key() {
        let body = SecretBytes::new(br#"{"webhook_secret":"abc","token":"def"}"#.to_vec());

        let secret = afd_vault::bag::parse::<WebhookSecret>(&body)
            .expect("the field is a non-empty string")
            .0;

        assert_eq!(
            secret.len(),
            3,
            "only the field's own bytes are key material"
        );
    }

    /// A fleet may spell its credential name longer than the vault holds. That
    /// is a misconfiguration to fail closed on, found without a read and
    /// without an error line per delivery.
    #[tokio::test]
    async fn a_credential_name_the_vault_refuses_is_no_secret_and_leaves_no_line() {
        let database = afd_db::test_util::unreachable_db();
        let vault = afd_vault::Vault::new(
            database.clone(),
            Arc::new(Kek::from_bytes([0x2a; 32])),
            Entropy::new(),
        );
        let admissions = afd_admission::Admissions::for_tests(
            database.clone(),
            afd_bus::test_util::unreachable(),
        );
        let ingress = Ingress::new(database, vault, admissions);
        let workspace =
            Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011").expect("a fixture identifier");

        // After construction: building the pool logs its own line.
        let capture = af_core::test_util::trace::Capture::install();
        for key in ["", &"n".repeat(65)] {
            let secret = ingress
                .platform_secret(&workspace, key)
                .await
                .expect("a refused name is no secret, not a failure");

            assert!(secret.is_none(), "{key:?}");
        }
        let events = capture.events();
        assert!(
            events.is_empty(),
            "a fleet's own document is no incident: {events:?}"
        );
    }
}
