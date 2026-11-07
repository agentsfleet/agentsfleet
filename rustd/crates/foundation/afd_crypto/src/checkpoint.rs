//! Scoped object keys and bounded-buffer encryption for private checkpoints.
//!
//! The daemon wraps each object key with the existing vault envelope. Trusted
//! transfer code receives only that temporary key and its registered nonce.
//! Native registration owns nonce uniqueness and immutable replay descriptors.

use aes_gcm::aead::{AeadInOut, Nonce, Tag, inout::InOutBuf};
use aes_gcm::{Aes256Gcm, KeyInit};
use afd_core::id::Uuid7;
use zeroize::{Zeroize, Zeroizing};

use crate::aad::Aad;
use crate::entropy::Entropy;
use crate::envelope::{Envelope, Sealer};
use crate::error::{Error, ErrorKind, Result};
use crate::secret::{Dek, Kek, SecretBytes};
use crate::{KEY_LEN, NONCE_LEN, TAG_LEN};

/// Native ownership and format already checked by checkpoint registration.
#[derive(Debug, Clone, Copy)]
pub struct CheckpointScope<'a> {
    /// Owning workspace, independent of the runner handling this attempt.
    pub workspace_id: &'a Uuid7,
    /// Canonical repository identity from the native registration.
    pub repository: &'a str,
    /// Random native object identity; a content hash cannot choose this value.
    pub object_id: &'a Uuid7,
    /// Checkpoint format whose objects this scope admits.
    pub format_version: u32,
}

/// Encrypted object-key metadata safe to retain with the private object record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedCheckpointKey {
    envelope: Envelope,
    nonce: [u8; NONCE_LEN],
}

/// One registration's temporary key; dropping the value wipes its owned bytes.
pub struct PreparedCheckpointKey {
    wrapped: WrappedCheckpointKey,
    temporary: SecretBytes,
}

impl std::fmt::Debug for PreparedCheckpointKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedCheckpointKey(redacted)")
    }
}

impl PreparedCheckpointKey {
    /// Creates the object key and fixed nonce before any upload is authorized.
    ///
    /// # Errors
    /// Returns entropy or envelope failures.
    pub fn create(
        entropy: &Entropy,
        sealer: &Sealer,
        master: &Kek,
        scope: CheckpointScope<'_>,
    ) -> Result<Self> {
        let mut bytes = Zeroizing::new([0_u8; KEY_LEN]);
        entropy.fill(bytes.as_mut_slice())?;
        let mut nonce = [0_u8; NONCE_LEN];
        entropy.fill(&mut nonce)?;
        let binding = aad(scope, "key", Some(crate::KEK_VERSION), Some(&nonce));
        let envelope = sealer.seal(master, &binding, bytes.as_slice())?;
        Ok(Self {
            wrapped: WrappedCheckpointKey { envelope, nonce },
            temporary: SecretBytes::new(bytes.to_vec()),
        })
    }

    /// Moves the stored descriptor and temporary material to their separate owners.
    #[must_use]
    pub fn into_parts(self) -> (WrappedCheckpointKey, SecretBytes) {
        (self.wrapped, self.temporary)
    }
}

impl WrappedCheckpointKey {
    /// Rebuilds metadata after the existing envelope parser checks its columns.
    #[must_use]
    pub const fn from_parts(envelope: Envelope, nonce: [u8; NONCE_LEN]) -> Self {
        Self { envelope, nonce }
    }

    /// The existing versioned envelope, stored without plaintext key material.
    #[must_use]
    pub const fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    /// Fixed registered nonce; upload retries reuse the identical ciphertext.
    #[must_use]
    pub const fn nonce(&self) -> &[u8; NONCE_LEN] {
        &self.nonce
    }

    /// Opens an authorized object's key without changing its ciphertext or nonce.
    ///
    /// # Errors
    /// Returns the shared opaque authentication failure or malformed key length.
    pub fn open(&self, master: &Kek, scope: CheckpointScope<'_>) -> Result<SecretBytes> {
        let binding = aad(
            scope,
            "key",
            Some(self.envelope.kek_version()),
            Some(&self.nonce),
        );
        let temporary = self.envelope.open(master, &binding)?;
        let _checked = Dek::from_slice(temporary.expose())?;
        Ok(temporary)
    }
}

/// Ciphertext and detached tag; transfer code can move the buffer without cloning.
#[derive(PartialEq, Eq)]
pub struct EncryptedCheckpointObject {
    ciphertext: Vec<u8>,
    tag: [u8; TAG_LEN],
}

impl std::fmt::Debug for EncryptedCheckpointObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedCheckpointObject")
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish_non_exhaustive()
    }
}

impl EncryptedCheckpointObject {
    /// Rebuilds an object whose lengths and digests the transfer owner checked.
    #[must_use]
    pub const fn from_parts(ciphertext: Vec<u8>, tag: [u8; TAG_LEN]) -> Self {
        Self { ciphertext, tag }
    }

    /// Moves the upload buffer and tag to storage without copying object bytes.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, [u8; TAG_LEN]) {
        (self.ciphertext, self.tag)
    }

    /// Encrypts a registered chunk in its existing allocation and drops the key.
    /// Native capture bounds the chunk before allocation and hashing.
    ///
    /// # Errors
    /// Returns malformed key length or opaque encryption failures.
    pub fn seal(
        key: SecretBytes,
        nonce: &[u8; NONCE_LEN],
        scope: CheckpointScope<'_>,
        mut plaintext: Zeroizing<Vec<u8>>,
    ) -> Result<Self> {
        let cipher = cipher(&key)?;
        drop(key);
        let binding = aad(scope, "data", None, None);
        let tag = cipher
            .encrypt_inout_detached(
                &Nonce::<Aes256Gcm>::from(*nonce),
                binding.as_bytes(),
                InOutBuf::from(plaintext.as_mut_slice()),
            )
            .map_err(|_err| Error::from(ErrorKind::OpenFailed))?;
        // A reused capture buffer can retain older plaintext beyond its length.
        plaintext.spare_capacity_mut().zeroize();
        Ok(Self {
            ciphertext: std::mem::take(&mut *plaintext),
            tag: tag.into(),
        })
    }

    /// Authenticates before returning plaintext; failures wipe the owned buffer.
    /// The transfer owner separately verifies both registered content digests.
    ///
    /// # Errors
    /// Returns malformed key length or the shared opaque open failure.
    pub fn open(
        self,
        key: SecretBytes,
        nonce: &[u8; NONCE_LEN],
        scope: CheckpointScope<'_>,
    ) -> Result<SecretBytes> {
        let mut buffer = Zeroizing::new(self.ciphertext);
        let cipher = cipher(&key)?;
        drop(key);
        let binding = aad(scope, "data", None, None);
        cipher
            .decrypt_inout_detached(
                &Nonce::<Aes256Gcm>::from(*nonce),
                binding.as_bytes(),
                InOutBuf::from(buffer.as_mut_slice()),
                &Tag::<Aes256Gcm>::from(self.tag),
            )
            .map_err(|_err| Error::from(ErrorKind::OpenFailed))?;
        Ok(SecretBytes::new(std::mem::take(&mut *buffer)))
    }
}

fn cipher(key: &SecretBytes) -> Result<Aes256Gcm> {
    let checked = Dek::from_slice(key.expose())?;
    Ok(Aes256Gcm::new(checked.expose().into()))
}

fn aad(
    scope: CheckpointScope<'_>,
    purpose: &str,
    version: Option<i32>,
    nonce: Option<&[u8; NONCE_LEN]>,
) -> Aad {
    let repository = hex::encode(scope.repository.as_bytes());
    let format = scope.format_version.to_string();
    let version = version.map(|value| value.to_string()).unwrap_or_default();
    let nonce = nonce.map(hex::encode).unwrap_or_default();
    // Every field excludes NUL: repository bytes use hex, identifiers are
    // checked UUIDs, and the remaining fields are fixed labels or numbers.
    let fields = [
        purpose,
        scope.workspace_id.as_str(),
        repository.as_str(),
        scope.object_id.as_str(),
        format.as_str(),
        version.as_str(),
        nonce.as_str(),
    ];
    let mut bytes = b"\0agentsfleet.checkpoint.v1\0".to_vec();
    for field in fields {
        bytes.extend_from_slice(field.as_bytes());
        bytes.push(0);
    }
    Aad::checkpoint(bytes)
}
