//! Named `SQLx` storage projection shared by every vault-opening path.
use crate::error::Result;
use afd_crypto::envelope::Envelope;

/// Ciphertext columns, independent of their position in a SELECT projection.
#[derive(Debug, sqlx::FromRow)]
pub struct StoredEnvelope {
    dek_ciphertext: Vec<u8>,
    dek_nonce: Vec<u8>,
    dek_tag: Vec<u8>,
    secret_nonce: Vec<u8>,
    secret_ciphertext: Vec<u8>,
    secret_tag: Vec<u8>,
    kek_version: i32,
}

impl StoredEnvelope {
    /// Checks component widths and the KEK version using the cryptographic owner.
    ///
    /// # Errors
    /// Returns the crypto error for an unsupported version or malformed component.
    pub fn into_envelope(self) -> Result<Envelope, afd_crypto::error::Error> {
        Envelope::from_parts(
            self.dek_ciphertext,
            &self.dek_nonce,
            &self.dek_tag,
            &self.secret_nonce,
            self.secret_ciphertext,
            &self.secret_tag,
            self.kek_version,
        )
    }
}
