//! One credential out of the vault: the row, the envelope, the plaintext.
//!
//! # What does NOT come across from `crypto_store.zig`
//!
//! That file is two hundred and sixty-six lines and this one is a fraction of
//! it, because most of what it does is already done elsewhere or does not
//! belong on this path:
//!
//! - The envelope layout, the two AEAD opens and every fixed-width length check
//!   are [`afd_crypto::envelope::Envelope`]'s, proven against published NIST
//!   vectors and against the Zig's own assertions.
//! - `loadAllForWorkspace` / `loadManyForWorkspace` and their per-row
//!   degradation belong to the secrets MAP (`secrets_resolve.zig`), which is a
//!   separate slice of §2. A provider key is one named row; reading a whole
//!   workspace to find it would be the wrong statement.
//! - The `decrypt_tally` and its `noteDecrypt` funnel exist to prove the
//!   LIBRARY read paths never decrypt. Nothing in this crate is a library read
//!   path — resolution decrypts by definition — so the tally has nothing to
//!   assert here and is not carried.
//!
//! What is left is a row read, a rebuild, and an open. Three statements of
//! plumbing, which is the right size for the part that is genuinely this
//! module's.
//!
//! # A missing row is `Ok(None)`
//!
//! `crypto_store.load` answers `SecretError.NotFound` and each caller catches
//! it and turns it into its own error — `PlatformKeyMissing` on the platform
//! path, `SecretMissing` on the self-managed one. The two callers want
//! different words for the same absence, which is exactly the shape an
//! `Option` has and an error does not. So absence arrives as `None` and the
//! naming is the caller's, one line up, instead of a `catch` in each.

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Envelope;
use afd_crypto::secret::SecretBytes;
use sqlx::Row as _;

use crate::error::{Result, query, vault_open};
use crate::provider::store::Providers;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_SECRET: &str = "vault credential";

/// Where one credential is held.
///
/// A pair rather than two arguments, because both are strings and they compile
/// clean in either order — and getting them the wrong way round would ask the
/// vault for a workspace named after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRef<'a> {
    /// The workspace holding the row.
    pub workspace_id: &'a Uuid7,
    /// The row's name within it.
    pub name: &'a str,
}

impl Providers {
    /// The plaintext of the credential at `key`, or nothing if none is held.
    ///
    /// The recovered bytes are [`SecretBytes`], which zeroes them when it goes
    /// out of scope. Everything derived from them inherits that obligation —
    /// see [`super::Resolved`] for how the key itself keeps it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row whose ciphertext
    /// columns are not a well-formed envelope, and an envelope that does not
    /// authenticate. The last two are deliberately indistinguishable to a
    /// caller: which check failed is an oracle, and the operator gets the
    /// distinction in the log instead.
    pub(crate) async fn open_secret(&self, key: KeyRef<'_>) -> Result<Option<SecretBytes>> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::provider::SELECT_SECRET)
            .bind(key.workspace_id.as_str())
            .bind(key.name)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SECRET))?;
        let Some(row) = row else {
            return Ok(None);
        };

        // Positional rather than by name, because the ORDER is the contract
        // this shares with `openEnvelopeAt`: the statement's projection and
        // `from_parts`' parameter list are the same six components in the same
        // sequence, and reading them by name would hide a projection that had
        // drifted out of that order.
        let column = |index: usize| {
            row.try_get::<Vec<u8>, _>(index)
                .map_err(query(CONTEXT_SECRET))
        };
        let envelope = Envelope::from_parts(
            column(0)?,
            &column(1)?,
            &column(2)?,
            &column(3)?,
            column(4)?,
            &column(5)?,
            row.try_get(6).map_err(query(CONTEXT_SECRET))?,
        )
        .map_err(vault_open)?;

        envelope
            .open(self.kek(), &Aad::new(key.workspace_id.as_str(), key.name))
            .map(Some)
            .map_err(vault_open)
    }
}
