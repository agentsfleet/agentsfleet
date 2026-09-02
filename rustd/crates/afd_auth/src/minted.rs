//! A credential: minted once, stored as a digest, never re-readable.
//!
//! # Why this lives beside the classifier and not beside the stores
//!
//! It is built from exactly two things this crate owns — [`CredentialKind`],
//! which supplies the marker, and [`Digest`], which is what a row stores — and
//! one thing `afd_crypto` owns. Keeping it in the domain crate that WRITES the
//! rows meant every store touching a credential had to depend on that whole
//! crate to draw one. Here, minting sits with classification: the two halves of
//! one fact, which is that a credential's class is carried by its bytes.
//!
//! Three credential classes are drawn this way — the runner's `agt_r`, the
//! tenant's `agt_t`, and the command line's `afc_` — and the Zig daemon draws
//! each of them in its own file, with its own `TOKEN_RANDOM_BYTES`, its own
//! `bytesToHex`, and its own concatenation. Three copies of one procedure is
//! three places for a digest to be taken over the wrong bytes, and the failure
//! has no test: it stores a value no lookup will ever match, and the class it
//! belongs to simply cannot log in.
//!
//! Here it is one type taking the class's marker. What varies between the three
//! is the marker and nothing else, which is what the parameter says.

use afd_crypto::entropy::Entropy;
use std::fmt;

use crate::credential::CredentialKind;
use crate::directory::Digest;

/// Random bytes behind a runner token's body.
///
/// `register.zig`'s `TOKEN_RANDOM_BYTES`, and the same 32 the other two
/// credential minters draw — the number `afd_auth::authenticate` already
/// depends on, since its shape check expects exactly 64 hex characters after
/// the marker. One constant now, where Zig has three that agree by luck.
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
    /// Draws a new credential of `kind`.
    ///
    /// The marker comes from [`CredentialKind`] rather than from the caller, so
    /// a mint cannot produce a value the authenticator would classify as a
    /// different class — or as no class at all.
    ///
    /// # Errors
    /// Returns an entropy failure when the host cannot produce random bytes. It
    /// is not degraded to a weaker source: a predictable credential is one an
    /// attacker mints for themselves.
    pub fn draw(kind: CredentialKind, entropy: &Entropy) -> afd_crypto::error::Result<Self> {
        let mut raw = [0u8; TOKEN_RANDOM_BYTES];
        entropy.fill(&mut raw)?;
        // Every class this crate mints carries a marker; the session bearer is
        // the only kind that does not, and nothing mints one of those — the
        // identity provider does.
        let marker = kind.prefix().unwrap_or_default();
        // `hex` rather than a `write!` loop: the encoding is not this crate's
        // problem, and a hand-written one is a place for a `{:x}` to lose a
        // leading zero. Lower-case, matching every stored credential column.
        let token: Box<str> = format!("{marker}{}", hex::encode(raw)).into();
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

#[cfg(test)]
mod tests {
    use afd_crypto::entropy::Entropy;

    use super::Minted;
    use crate::credential::CredentialKind;

    /// The debug rendering never carries the credential it wraps.
    ///
    /// This is the whole reason the impl is hand-written: a `#[derive(Debug)]`
    /// on any struct that transitively holds a `Minted` is how a live
    /// credential reaches a log line, and nothing downstream would notice —
    /// the log is written, shipped and indexed exactly as it would be for a
    /// redacted one.
    ///
    /// So the assertion is the negative: the exposed token must not appear,
    /// and neither must a leading fragment of it, because a truncating
    /// formatter would still hand an attacker the search space.
    #[test]
    fn the_debug_rendering_never_carries_the_token() -> Result<(), &'static str> {
        for kind in [
            CredentialKind::TenantApiKey,
            CredentialKind::CliCredential,
            CredentialKind::RunnerToken,
        ] {
            let minted = Minted::draw(kind, &Entropy::new())
                .map_err(|_drained| "the host draws entropy for a credential")?;
            let token = minted.expose().to_owned();
            let rendered = format!("{minted:?}");

            assert!(
                !rendered.contains(&token),
                "the whole token must not render"
            );
            assert!(
                !rendered.contains(&token[..16]),
                "nor a leading fragment of it — a truncated credential is still \
                 a search space handed over"
            );
            assert!(
                rendered.contains("redacted"),
                "and the reader is told the value was withheld rather than absent"
            );
        }
        Ok(())
    }
}
