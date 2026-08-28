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
use crate::secret::{Kek, SecretBytes};

/// Bytes in an HMAC-SHA256 output.
pub const HMAC_SHA256_TAG_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// Why the two keying constructors below cannot fail.
///
/// Named once rather than spelled at each `expect`: RULE UFS reads a repeated
/// literal as two facts that can drift, and this is one fact about HMAC stated
/// in two places.
const KEY_LENGTH_IS_UNCONSTRAINED: &str = "HMAC accepts a key of any length";

/// A message authentication code, comparable only in constant time.
///
/// `PartialEq` is deliberately NOT derived. Deriving it would give callers a
/// `==` that short-circuits, which is the timing leak this type exists to
/// prevent; [`HmacSha256Tag::verify`] is the only way to compare two of these.
#[derive(Clone)]
pub struct HmacSha256Tag([u8; HMAC_SHA256_TAG_LEN]);

impl HmacSha256Tag {
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
            .expect(KEY_LENGTH_IS_UNCONSTRAINED);
        mac.update(message);
        Self(mac.finalize().into_bytes().into())
    }

    /// Computes the code over `parts` under a key of any length.
    ///
    /// The device-flow pepper is an operator-supplied string rather than a
    /// key-encryption key, so it has no fixed width. HMAC is defined for a key
    /// of any length — the construction pads or hashes it down itself — which
    /// is why [`HmacSha256Tag::compute`] can take a fixed array and this can take a
    /// slice with no second implementation between them.
    ///
    /// `parts` are fed in order with no separator, matching what the Zig
    /// daemon signs. Both binaries write the same Redis blob and a Lua script
    /// compares the two hex renderings as text, so this is a DATA FORMAT and
    /// not a choice — a separator added here would invalidate every session the
    /// other binary approved.
    ///
    /// # Panics
    /// Cannot, for the reason [`HmacSha256Tag::compute`] cannot: HMAC forbids no key
    /// length, so `new_from_slice`'s error arm is unconstructible.
    #[must_use]
    pub fn compute_peppered(pepper: &SecretBytes, parts: &[&[u8]]) -> Self {
        #[expect(
            clippy::expect_used,
            reason = "HMAC accepts a key of any length, so the error arm cannot be constructed; \
                      see the doc comment above"
        )]
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(pepper.expose())
            .expect(KEY_LENGTH_IS_UNCONSTRAINED);
        for part in parts {
            mac.update(part);
        }
        Self(mac.finalize().into_bytes().into())
    }

    /// Rebuilds a code from stored bytes, rejecting a wrong length.
    ///
    /// # Errors
    /// Returns a malformed-envelope error when the slice is not `HMAC_SHA256_TAG_LEN` bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let sized: [u8; HMAC_SHA256_TAG_LEN] = bytes.try_into().map_err(|_err| {
            Error::new(ErrorKind::ComponentLength {
                component: "message authentication code",
                expected: HMAC_SHA256_TAG_LEN,
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
    pub const fn as_bytes(&self) -> &[u8; HMAC_SHA256_TAG_LEN] {
        &self.0
    }

    /// The code as lowercase hexadecimal, the form the daemon stores.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(HMAC_SHA256_TAG_LEN * 2);
        for byte in self.0 {
            // A two-digit lowercase hex write cannot fail into a `String`.
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

/// Renders the code, which is a digest rather than the secret behind it.
impl std::fmt::Debug for HmacSha256Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HmacSha256Tag({})", self.to_hex())
    }
}
