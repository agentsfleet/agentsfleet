//! Opening one stored secret, for the daemon's own use.
//!
//! # This is not the tenant surface, and the distinction is the whole design
//!
//! The four workspace verbs — create, list, replace, delete — still return no
//! stored value to anybody, and [`crate::Directory`] still cannot decrypt. What
//! lands here is different in kind: an ingress path that has to CHECK a
//! signature needs the shared secret to compute a tag with, and no amount of
//! projection metadata substitutes for the bytes. The signed delivery never
//! sees them, and neither does any response body.
//!
//! So the rule is not "the vault never opens an envelope". It is:
//!
//! - No ROUTE returns a stored secret. Still true — nothing here is reachable
//!   from a handler that serializes what it gets back.
//! - The LIST performs zero decrypts. Still true, and still proved twice: by
//!   `Directory` holding no key and by the projection statement carrying no
//!   ciphertext column.
//! - Opening an envelope is [`Vault`]'s alone, because [`Vault`] is the half
//!   that holds the key. This module is on that half deliberately, so the
//!   read-and-delete half's inability to decrypt stays a property of its type.
//!
//! `crypto_store.zig`'s `load` is the same verb, reached the same way, and the
//! Zig webhook lookup calls it for exactly this reason.

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Envelope;
use afd_crypto::secret::SecretBytes;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::secret::SecretName;
use crate::{Vault, sql};

/// The context a failed load reports under.
const CONTEXT_LOAD: &str = "open a stored secret";

impl Vault {
    /// The plaintext of one secret this workspace holds.
    ///
    /// `Ok(None)` for a name the workspace does not hold — indistinguishable
    /// from one that never existed, because the statement is scoped to the
    /// workspace and another tenant's name resolves no row rather than the
    /// wrong row.
    ///
    /// The bytes come back as [`SecretBytes`], which zeroes on drop. A caller
    /// that copies them into a `String` has left that guarantee behind and owns
    /// what happens next.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row whose envelope columns
    /// are not the widths this build reads, and an envelope that would not
    /// open. The last is deliberately one error however it failed — see
    /// `afd_crypto::error` on why the two AEAD layers are indistinguishable.
    pub async fn load(&self, workspace: &Uuid7, name: &SecretName) -> Result<Option<SecretBytes>> {
        let mut connection = self.directory.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_SECRET_ENVELOPE)
            .bind(workspace.as_str())
            .bind(name.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(query(CONTEXT_LOAD))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let unreadable = query(CONTEXT_LOAD);
        let envelope = Envelope::from_parts(
            row.try_get(0).map_err(&unreadable)?,
            row.try_get::<Vec<u8>, _>(1)
                .map_err(&unreadable)?
                .as_slice(),
            row.try_get::<Vec<u8>, _>(2)
                .map_err(&unreadable)?
                .as_slice(),
            row.try_get::<Vec<u8>, _>(3)
                .map_err(&unreadable)?
                .as_slice(),
            row.try_get(4).map_err(&unreadable)?,
            row.try_get::<Vec<u8>, _>(5)
                .map_err(&unreadable)?
                .as_slice(),
            row.try_get(6).map_err(&unreadable)?,
        )?;

        // The same associated data the seal bound, so a ciphertext moved to
        // another workspace or another name does not open. That binding is the
        // reason the row's own identity is an input rather than a comment.
        let opened = envelope.open(&self.kek, &Aad::new(workspace.as_str(), name.as_str()))?;

        // No secret material in the line, and no length either: a length is a
        // narrowing fact about a key.
        tracing::debug!(
            workspace = workspace.as_str(),
            name = name.as_str(),
            event = "secret_opened",
        );
        Ok(Some(opened))
    }
}
