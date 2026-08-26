//! Taking a session token apart, before any key is involved.
//!
//! Split from the verifier because the two fail for different reasons and are
//! testable at different costs: everything here is a pure function over bytes,
//! so the malformed-token branches need no key set, no clock and no network.
//!
//! # What is deliberately NOT here
//!
//! Nothing decides whether a token is ACCEPTABLE. This module answers "what
//! does it say", and every claim check — issuer, audience, expiry — lives in
//! the verifier where the configuration and the clock are. Splitting it the
//! other way would put policy in a parser.

use afd_auth::verifier::VerifyError;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// The three base64url segments of a compact JWS, still encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Segments<'a> {
    /// The encoded header, kept encoded because it is half the signing input.
    pub(crate) header: &'a str,
    /// The encoded payload, for the same reason.
    pub(crate) payload: &'a str,
    /// The encoded signature.
    pub(crate) signature: &'a str,
}

impl<'a> Segments<'a> {
    /// Splits `token` on its two dots.
    ///
    /// # Errors
    /// [`VerifyError::Malformed`] unless there are exactly three non-empty
    /// segments. Exactly three: a four-segment value is JWE, which this daemon
    /// does not accept, and accepting it here would hand an encrypted token to
    /// a signature verifier that would then read its ciphertext as a payload.
    pub(crate) fn split(token: &'a str) -> Result<Self, VerifyError> {
        let mut parts = token.split('.');
        let (Some(header), Some(payload), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(VerifyError::Malformed);
        };
        if header.is_empty() || payload.is_empty() || signature.is_empty() {
            return Err(VerifyError::Malformed);
        }
        Ok(Self {
            header,
            payload,
            signature,
        })
    }

    /// The bytes the signature is over: `header.payload`, exactly as received.
    ///
    /// Re-encoding either segment would change these bytes, so the ENCODED
    /// forms are what this carries. That is why [`Segments`] borrows rather
    /// than decoding eagerly.
    pub(crate) fn signing_input(self) -> String {
        let mut input = String::with_capacity(self.header.len() + 1 + self.payload.len());
        input.push_str(self.header);
        input.push('.');
        input.push_str(self.payload);
        input
    }
}

/// Decodes one base64url segment, without padding.
///
/// # Errors
/// [`VerifyError::Malformed`]. A segment that does not decode is a malformed
/// token and nothing else — never a signature failure, which would suggest to
/// an operator that a key had rotated.
pub(crate) fn decode_segment(segment: &str) -> Result<Vec<u8>, VerifyError> {
    URL_SAFE_NO_PAD
        .decode(segment)
        .map_err(|_invalid| VerifyError::Malformed)
}

/// The header fields this daemon reads.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Header {
    /// The signing algorithm. Only `RS256` is accepted.
    pub(crate) alg: String,
    /// The key identifier, which selects a key from the set.
    pub(crate) kid: Option<String>,
}

/// The only signing algorithm this daemon accepts.
///
/// An ALLOWLIST of exactly one, not a blocklist of the dangerous ones. That is
/// what makes `alg: "none"` — the algorithm-confusion attack, where a token
/// declares itself unsigned and a lenient verifier agrees — unreachable by
/// construction rather than by remembering to exclude it.
pub(crate) const ACCEPTED_ALG: &str = "RS256";

impl Header {
    /// Parses and validates a decoded header.
    ///
    /// # Errors
    /// - [`VerifyError::Malformed`] when it is not a JSON object with an `alg`.
    /// - [`VerifyError::UnsupportedAlgorithm`] when `alg` is not `RS256`.
    /// - [`VerifyError::MissingKeyId`] when there is no `kid` to select a key.
    pub(crate) fn parse(raw: &[u8]) -> Result<(Self, String), VerifyError> {
        let header: Self =
            afd_core::json::object_from_slice(raw).map_err(|_invalid| VerifyError::Malformed)?;
        if header.alg != ACCEPTED_ALG {
            return Err(VerifyError::UnsupportedAlgorithm);
        }
        let kid = header.kid.clone().ok_or(VerifyError::MissingKeyId)?;
        Ok((header, kid))
    }
}
