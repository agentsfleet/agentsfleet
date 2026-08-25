//! HMAC-SHA256 over a canonical byte string, compared in constant time.
//!
//! Used where the daemon must recognise a value it issued — an API-key hash, a
//! session code — without storing the value itself. The comparison is the part
//! that matters: a byte-by-byte `==` returns early on the first difference, and
//! the time it takes leaks how much of a guess was correct. RULE CTM in
//! `docs/greptile-learnings/RULES.md` exists because of exactly that.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::{Error, ErrorKind, Result};
use crate::secret::Kek;

/// Bytes in an HMAC-SHA256 output.
pub const MAC_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// A message authentication code, comparable only in constant time.
///
/// `PartialEq` is deliberately NOT derived. Deriving it would give callers a
/// `==` that short-circuits, which is the timing leak this type exists to
/// prevent; [`Mac256::verify`] is the only way to compare two of these.
#[derive(Clone)]
pub struct Mac256([u8; MAC_LEN]);

impl Mac256 {
    /// Computes the code over `message` under `key`.
    ///
    /// Infallible, and deliberately so. `new_from_slice` is typed as fallible
    /// because `KeyInit` is shared with fixed-key ciphers; HMAC is defined for a
    /// key of ANY length, and this one is a `[u8; KEY_LEN]` besides, so the
    /// error arm is unreachable by construction rather than merely unlikely.
    ///
    /// An earlier revision returned `Result` to avoid the override below. That
    /// bought a branch no test could ever enter — dead code wearing a safety
    /// jacket — and pushed an impossible error onto every caller. The workspace
    /// denies `expect` because these crates link into a daemon that must not
    /// abort on a malformed WIRE PAYLOAD; a compile-time-fixed key length is not
    /// one, so `M-LINT-OVERRIDE-EXPECT` applies and the override is scoped to
    /// this single expression with its reason attached.
    ///
    /// # Panics
    /// Cannot. `new_from_slice` rejects only a key length its algorithm forbids,
    /// and HMAC forbids none — the reason the arm is `expect` rather than a
    /// propagated error is the whole subject of the paragraphs above.
    #[must_use]
    pub fn compute(key: &Kek, message: &[u8]) -> Self {
        #[expect(
            clippy::expect_used,
            reason = "HMAC accepts a key of any length and this one is a fixed-width array, \
                      so the error arm cannot be constructed; see the doc comment above"
        )]
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key.expose().as_slice())
            .expect("HMAC accepts a key of any length");
        mac.update(message);
        Self(mac.finalize().into_bytes().into())
    }

    /// Rebuilds a code from stored bytes, rejecting a wrong length.
    ///
    /// # Errors
    /// Returns a malformed-envelope error when the slice is not `MAC_LEN` bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let sized: [u8; MAC_LEN] = bytes.try_into().map_err(|_err| {
            Error::new(ErrorKind::ComponentLength {
                component: "message authentication code",
                expected: MAC_LEN,
                actual: bytes.len(),
            })
        })?;
        Ok(Self(sized))
    }

    /// Compares two codes in time independent of where they first differ.
    ///
    /// # Errors
    /// Returns a MAC-mismatch error when the codes are not equal.
    pub fn verify(&self, other: &Self) -> Result<()> {
        if self.0.ct_eq(&other.0).into() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::MacMismatch))
        }
    }

    /// The code as bytes, for storage.
    ///
    /// A code is not secret — it is what gets stored so the secret does not
    /// have to be — so this returns the bytes rather than redacting them.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; MAC_LEN] {
        &self.0
    }

    /// The code as lowercase hexadecimal, the form the daemon stores.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(MAC_LEN * 2);
        for byte in self.0 {
            // A two-digit lowercase hex write cannot fail into a `String`.
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

/// Renders the code, which is a digest rather than the secret behind it.
impl std::fmt::Debug for Mac256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mac256({})", self.to_hex())
    }
}
