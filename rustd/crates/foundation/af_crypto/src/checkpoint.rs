//! One workspace key seals every checkpoint object, behind a cipher the deployment selects.
//!
//! The key is used as the vault stores it: it seals each file chunk and the
//! turn history root, and it keys each chunk's content identity. Every object
//! of a workspace therefore shares one key, so each carries its own random
//! 96-bit nonce, registered before its first upload link.

use af_core::id::Uuid7;
use aws_lc_rs::aead::{Aad as AeadAad, LessSafeKey, Nonce};
use zeroize::{Zeroize, Zeroizing};

use crate::aad::Aad;
use crate::error::{Error, ErrorKind, Result};
use crate::mac::HmacSha256Tag;
use crate::secret::{Dek, SecretBytes};
use crate::{NONCE_LEN, TAG_LEN};

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

impl CheckpointScope<'_> {
    /// The associated data a chunk is sealed under: a chunk moved to another
    /// workspace, repository, object or format fails its tag.
    #[must_use]
    pub fn binding(&self) -> Aad {
        let repository = hex::encode(self.repository.as_bytes());
        let format = self.format_version.to_string();
        // Every field excludes NUL: repository bytes use hex, identifiers are
        // checked UUIDs, and the remaining fields are fixed labels or numbers.
        // The two empty fields close the format the external vector was
        // produced under (`tests/checkpoint.rs`).
        let fields = [
            "data",
            self.workspace_id.as_str(),
            repository.as_str(),
            self.object_id.as_str(),
            format.as_str(),
            "",
            "",
        ];
        let mut bytes = b"\0agentsfleet.checkpoint.v1\0".to_vec();
        for field in fields {
            bytes.extend_from_slice(field.as_bytes());
            bytes.push(0);
        }
        Aad::checkpoint(bytes)
    }
}

/// Seals and opens one checkpoint object under the workspace's key.
///
/// The sealed form is what the bucket or the checkpoint row stores. Both
/// implementations refuse a key that is not an AES-256 key, so minting,
/// loading and handing the key over fail alike under either.
pub trait ChunkCipher: Send + Sync + std::fmt::Debug {
    /// Bytes a sealed object carries beyond its plaintext.
    fn overhead(&self) -> usize;

    /// Seals `plaintext` in its own allocation and returns the stored form.
    ///
    /// # Errors
    /// A malformed key length, or the shared opaque encryption failure.
    fn seal(
        &self,
        key: &SecretBytes,
        nonce: &[u8; NONCE_LEN],
        binding: &Aad,
        plaintext: Zeroizing<Vec<u8>>,
    ) -> Result<Vec<u8>>;

    /// Returns the plaintext of a stored object, in the stored allocation.
    ///
    /// # Errors
    /// A malformed key length, or the shared opaque open failure.
    fn open(
        &self,
        key: &SecretBytes,
        nonce: &[u8; NONCE_LEN],
        binding: &Aad,
        sealed: Vec<u8>,
    ) -> Result<SecretBytes>;
}

/// AES-256-GCM (Advanced Encryption Standard, Galois/Counter Mode): the
/// ciphertext, then its 16-byte authentication tag.
#[derive(Debug, Clone, Copy, Default)]
pub struct Aes256Gcm;

impl ChunkCipher for Aes256Gcm {
    fn overhead(&self) -> usize {
        TAG_LEN
    }

    fn seal(
        &self,
        key: &SecretBytes,
        nonce: &[u8; NONCE_LEN],
        binding: &Aad,
        mut plaintext: Zeroizing<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        // Detached, then appended once the buffer holds only ciphertext: a
        // growth for the tag may move the allocation, and what it leaves behind
        // is then ciphertext rather than plaintext.
        let tag = cipher(key)?
            .seal_in_place_separate_tag(
                Nonce::assume_unique_for_key(*nonce),
                AeadAad::from(binding.as_bytes()),
                plaintext.as_mut_slice(),
            )
            .map_err(|_err| Error::from(ErrorKind::OpenFailed))?;
        // A reused capture buffer can retain older plaintext beyond its length.
        plaintext.spare_capacity_mut().zeroize();
        let mut sealed = std::mem::take(&mut *plaintext);
        sealed.extend_from_slice(tag.as_ref());
        Ok(sealed)
    }

    fn open(
        &self,
        key: &SecretBytes,
        nonce: &[u8; NONCE_LEN],
        binding: &Aad,
        sealed: Vec<u8>,
    ) -> Result<SecretBytes> {
        // A failed open wipes whatever the buffer holds on its way out.
        let mut buffer = Zeroizing::new(sealed);
        // The trailing `TAG_LEN` bytes are the tag; a shorter object is refused.
        let length = cipher(key)?
            .open_in_place(
                Nonce::assume_unique_for_key(*nonce),
                AeadAad::from(binding.as_bytes()),
                buffer.as_mut_slice(),
            )
            .map_err(|_err| Error::from(ErrorKind::OpenFailed))?
            .len();
        buffer.truncate(length);
        Ok(SecretBytes::new(std::mem::take(&mut *buffer)))
    }
}

/// The bytes as they are, with no tag: a deployment measures a save and a
/// restore without encryption before it turns encryption on.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl ChunkCipher for Plain {
    fn overhead(&self) -> usize {
        0
    }

    fn seal(
        &self,
        key: &SecretBytes,
        _nonce: &[u8; NONCE_LEN],
        _binding: &Aad,
        mut plaintext: Zeroizing<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let _checked = Dek::from_slice(key.expose())?;
        Ok(std::mem::take(&mut *plaintext))
    }

    fn open(
        &self,
        key: &SecretBytes,
        _nonce: &[u8; NONCE_LEN],
        _binding: &Aad,
        sealed: Vec<u8>,
    ) -> Result<SecretBytes> {
        let _checked = Dek::from_slice(key.expose())?;
        Ok(SecretBytes::new(sealed))
    }
}

/// A chunk's content identity under its workspace's key: equal content within
/// one workspace has one identity, and no other workspace can compute it.
#[must_use]
pub fn content_identity(key: &SecretBytes, format_version: i32, sha256: &str, size: u64) -> String {
    HmacSha256Tag::compute_peppered(
        key,
        &[
            b"\0agentsfleet.content.v1\0",
            &format_version.to_be_bytes(),
            sha256.as_bytes(),
            &size.to_be_bytes(),
        ],
    )
    .to_hex()
}

fn cipher(key: &SecretBytes) -> Result<LessSafeKey> {
    let checked = Dek::from_slice(key.expose())?;
    crate::envelope::cipher(checked.expose())
}
