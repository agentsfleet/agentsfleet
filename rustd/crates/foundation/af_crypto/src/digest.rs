//! SHA-256, whole or streamed, for the content digests the workspace stores,
//! compares and publishes.
//!
//! Every first-party SHA-256 above this crate goes through here, so the
//! workspace computes one SHA-256: AWS-LC's. The output is the bare 32 bytes.
//! How a caller spells them — lowercase hex, base64url, a version prefix — is
//! that caller's stored format and stays beside it, where its pinned test is.

use std::fmt;
use std::io::{self, Write};

use aws_lc_rs::digest::{self, Context, SHA256};

/// Bytes in a SHA-256 digest.
pub const SHA256_LEN: usize = 32;

/// The SHA-256 of `bytes`.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; SHA256_LEN] {
    sized(&digest::digest(&SHA256, bytes))
}

/// A SHA-256 fed in parts: the digest of the parts' concatenation, in order,
/// with nothing added between them.
#[derive(Clone)]
pub struct Sha256(Context);

impl Sha256 {
    /// A hash that has read nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self(Context::new(&SHA256))
    }

    /// Feeds `bytes` after everything fed so far.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The digest of everything fed.
    #[must_use]
    pub fn finish(self) -> [u8; SHA256_LEN] {
        sized(&self.0.finish())
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// Names the hash without rendering its running state.
impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sha256")
    }
}

/// A writer that hashes exactly the bytes its inner writer accepted.
///
/// Each `write` hashes the prefix the inner writer reports written, and
/// nothing when it fails, so after a short write the caller's retry of the
/// rest is hashed once, and the digest is always the digest of what reached
/// `inner`. Wrapping [`io::sink`] hashes everything written to it.
#[derive(Debug)]
pub struct Sha256Writer<W> {
    inner: W,
    hash: Sha256,
}

impl<W> Sha256Writer<W> {
    /// Hashes what `inner` accepts from here on.
    #[must_use]
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            hash: Sha256::new(),
        }
    }

    /// The digest of every byte `inner` accepted, and `inner` back.
    #[must_use]
    pub fn finish(self) -> ([u8; SHA256_LEN], W) {
        (self.hash.finish(), self.inner)
    }
}

impl<W: Write> Write for Sha256Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        // `Write` forbids reporting more than `buf` holds; a writer that does
        // anyway is hashed for the bytes it was actually given.
        self.hash.update(buf.get(..written).unwrap_or(buf));
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The digest's bytes as an array; SHA-256 always yields `SHA256_LEN` of them.
fn sized(digest: &digest::Digest) -> [u8; SHA256_LEN] {
    let mut bytes = [0_u8; SHA256_LEN];
    bytes.copy_from_slice(digest.as_ref());
    bytes
}
