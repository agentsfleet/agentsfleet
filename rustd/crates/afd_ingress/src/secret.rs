//! Opening the one secret a signature is checked against.
//!
//! # Two stored shapes, and they are not interchangeable
//!
//! The per-fleet HMAC family stores a JSON OBJECT and signs with one field of
//! it, `webhook_secret`. The Svix family stores the raw `whsec_…` string with
//! no envelope of its own. `serve_webhook_lookup.zig` reads them with two
//! functions for that reason and so does this module — a single reader taking
//! an "is it JSON" branch would be one flag away from feeding a whole JSON
//! document to an HMAC as if it were a key.
//!
//! # Every failure here is `Ok(None)`, and that is the fail-closed answer
//!
//! A missing credential, a body that is not an object, an absent field, a field
//! that is not a string, an empty string: all of them resolve to no secret, and
//! no secret is [`afd_webhook::Refusal::Unconfigured`] one layer up — never a
//! pass, and never a comparison against an empty key. `Scheme::verify_at`
//! carries the same guard again on its own side, which is defence in depth
//! rather than duplication: an empty key makes the tag deterministic and
//! attacker-computable, so two independent refusals is the right number.
//!
//! Nothing here logs the secret, its length, or any prefix of it. A length is a
//! narrowing fact about a key (`docs/LOGGING_STANDARD.md` §6), and
//! [`afd_vault::Vault::load`] declines to log one for the same reason.

use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_vault::SecretName;

use crate::Ingress;
use crate::binding::Binding;
use crate::error::Result;

/// The field of a stored credential object that holds the shared secret.
///
/// `serve_webhook_lookup.zig`'s `WEBHOOK_SECRET_FIELD`, kept byte-for-byte: an
/// operator stores this credential once and both daemons read it during a
/// cutover, so the field name is a stored-data contract rather than a spelling
/// this crate may improve.
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
        let stored = self
            .stored_credential(binding.workspace(), binding.credential_name())
            .await?;
        Ok(stored.as_ref().and_then(webhook_secret_field))
    }

    /// The Svix signing secret, for a fleet whose trigger declares a ref.
    ///
    /// Answers the RAW stored bytes. A Svix secret is a `whsec_`-prefixed
    /// base64 string, and both the prefix strip and the decode belong to the
    /// verifier that knows the scheme — `afd_webhook::vendor::svix` — rather
    /// than to the reader that fetched it.
    ///
    /// # Errors
    /// As [`Self::signing_secret`].
    pub async fn svix_secret(&self, binding: &Binding) -> Result<Option<SecretBytes>> {
        let Some(signature) = binding.signature() else {
            return Ok(None);
        };
        self.stored_credential(binding.workspace(), signature.secret_ref())
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
    /// field, which is why this shares [`webhook_secret_field`] rather than
    /// carrying a second reader — `webhook_verify.zig:55` declares the App's
    /// `platform_secret_field` as that same name.
    ///
    /// `Ok(None)` for every way the secret can turn out to be unusable, and for
    /// a deployment that has configured no admin workspace at all. All of them
    /// are [`afd_webhook::Refusal::Unconfigured`] one layer up — never a pass.
    ///
    /// # Errors
    /// As [`Self::signing_secret`].
    pub async fn platform_secret(
        &self,
        admin_workspace: &Uuid7,
        key: &str,
    ) -> Result<Option<SecretBytes>> {
        let stored = self.stored_credential(admin_workspace, key).await?;
        Ok(stored.as_ref().and_then(webhook_secret_field))
    }

    /// One workspace credential, by key name, or nothing.
    ///
    /// A name the vault would refuse — empty, or past its bound — resolves to
    /// `None` rather than raising: it came from a fleet's stored document, so it
    /// is a misconfiguration to fail closed on, not an incident to wake an
    /// operator with at delivery time.
    async fn stored_credential(&self, workspace: &Uuid7, key: &str) -> Result<Option<SecretBytes>> {
        let Ok(name) = SecretName::parse(key) else {
            return Ok(None);
        };
        Ok(self.vault.load(workspace, &name).await?)
    }
}

/// The `webhook_secret` field of a stored credential object, as key material.
///
/// Re-wrapped into a fresh [`SecretBytes`] rather than handed out as a slice of
/// the parsed tree: `serde_json` owns that tree on the heap with no destructor
/// that zeroes it, so the copy this returns is the one carrying the guarantee.
fn webhook_secret_field(stored: &SecretBytes) -> Option<SecretBytes> {
    let document: serde_json::Value = serde_json::from_slice(stored.expose()).ok()?;
    let field = document.get(WEBHOOK_SECRET_FIELD)?.as_str()?;
    if field.is_empty() {
        return None;
    }
    Some(SecretBytes::new(field.as_bytes().to_vec()))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{SecretBytes, webhook_secret_field};

    /// Every shape a stored credential can take that names no usable secret.
    ///
    /// The Zig proves the same set on its Svix twin
    /// (`serve_webhook_lookup.zig`'s `extractSecretRef` suite). Each one of
    /// these reaching the wall as "no secret configured" is what makes the
    /// refusal `UZ-WH-020` rather than a tag computed against nothing.
    #[test]
    fn no_stored_shape_but_a_populated_field_yields_key_material() {
        let refused = [
            "not json at all",
            "[\"webhook_secret\"]",
            "\"webhook_secret\"",
            "{}",
            "{\"other\":\"x\"}",
            "{\"webhook_secret\":42}",
            "{\"webhook_secret\":null}",
            "{\"webhook_secret\":\"\"}",
        ];
        for stored in refused {
            let body = SecretBytes::new(stored.as_bytes().to_vec());
            assert!(
                webhook_secret_field(&body).is_none(),
                "`{stored}` names no usable secret and must fail closed"
            );
        }
    }

    #[test]
    fn the_field_comes_back_as_its_own_zeroing_copy() {
        let body = SecretBytes::new(br#"{"webhook_secret":"s3cr3t","other":"x"}"#.to_vec());

        let secret = webhook_secret_field(&body).expect("the field is a non-empty string");

        assert_eq!(secret.expose(), b"s3cr3t");
    }

    /// The sibling fields never leak into the key.
    ///
    /// A reader that handed back the whole document would still pass the test
    /// above's `is_some` check, and every signature would then fail to compare
    /// for a reason no log line would name.
    #[test]
    fn the_surrounding_document_is_not_the_key() {
        let body = SecretBytes::new(br#"{"webhook_secret":"abc","token":"def"}"#.to_vec());

        let secret = webhook_secret_field(&body).expect("the field is a non-empty string");

        assert_eq!(
            secret.len(),
            3,
            "only the field's own bytes are key material"
        );
    }
}
