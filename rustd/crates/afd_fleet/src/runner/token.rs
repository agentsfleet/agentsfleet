//! The `agt_r` credential: minted once, stored as a digest, never re-readable.

use afd_auth::credential::RUNNER_TOKEN_PREFIX;
use afd_auth::directory::Digest;
use afd_crypto::entropy::Entropy;
use std::fmt;

use crate::error::Result;

/// Random bytes behind a runner token's body.
///
/// `register.zig`'s `TOKEN_RANDOM_BYTES`, and the same 32 the other two
/// credential minters draw — the number `afd_auth::authenticate` already
/// depends on, since its shape check expects exactly 64 hex characters after
/// the marker.
const TOKEN_RANDOM_BYTES: usize = 32;

/// A freshly minted runner token, and the digest that will be stored for it.
///
/// The two travel together because they must not be computed apart: a digest
/// taken over anything but the WHOLE presented value — marker included —
/// authenticates nothing, and that is a mistake with no failing test, only a
/// fleet that cannot log in.
pub struct Minted {
    token: Box<str>,
    digest: Digest,
}

impl Minted {
    /// Draws a new `agt_r` credential.
    ///
    /// # Errors
    /// Returns an entropy failure when the host cannot produce random bytes. It
    /// is not degraded to a weaker source: a predictable runner token is a
    /// credential an attacker mints for themselves.
    pub fn draw(entropy: &Entropy) -> Result<Self> {
        let mut raw = [0u8; TOKEN_RANDOM_BYTES];
        entropy.fill(&mut raw)?;
        // `hex` rather than a `write!` loop: the encoding is not this crate's
        // problem, and a hand-written one is a place for a `{:x}` to lose a
        // leading zero. Lower-case, matching every stored credential column.
        let token: Box<str> = format!("{RUNNER_TOKEN_PREFIX}{}", hex::encode(raw)).into();
        // Over the WHOLE value, marker included, through the same function that
        // hashes what the holder later presents — a digest taken over anything
        // else stores a value no lookup will ever match.
        let digest = Digest::of_minted(&token);
        Ok(Self { token, digest })
    }

    /// The token, for the one response that reveals it.
    ///
    /// Named `expose` so every call site reads as a deliberate act — the
    /// convention `afd_crypto`'s secret newtypes and `afd_auth::Presented` both
    /// follow.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.token
    }

    /// The digest the row stores.
    #[must_use]
    pub const fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// Renders the length, never the value.
///
/// A `#[derive(Debug)]` on any struct that transitively holds a credential is
/// how the credential reaches a log.
impl fmt::Debug for Minted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Minted({} bytes, redacted)", self.token.len())
    }
}

/// Zeroes the token when the response that revealed it has been written.
///
/// A `Box<str>` is freed, not overwritten, so without this the credential sits
/// in released heap for as long as the allocator leaves it there. Hand-written
/// rather than derived, for the reason the workspace manifest records:
/// `zeroize_derive` is the last crate in this graph on `syn` 2.
impl Drop for Minted {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        let mut bytes = std::mem::take(&mut self.token).into_boxed_bytes();
        bytes.zeroize();
    }
}
