//! How each provider says "this body came from me".
//!
//! # A closed enum, deliberately not a table of parts
//!
//! The tempting shape is a struct of knobs — a prefix, a separator, where the
//! timestamp goes — because then a new provider is a new row and nobody writes
//! code. That shape is a signing grammar expressed as configuration, and a
//! signing grammar expressed as configuration is a small language whose
//! semantics live in whoever last edited a row. The canonicalisation is the
//! dangerous part of webhook verification; it is the part that must be READ.
//!
//! So each variant states its own basestring in code. A new provider is a new
//! arm, the compiler names every `match` that has to consider it, and a reviewer
//! diffing the arm sees exactly which bytes get signed.
//!
//! `webhook_verify.zig` takes the other road — `VerifyConfig` with
//! `includes_timestamp`, `hmac_version` and `prefix` fields — and the cost is
//! visible there: `verifyHmac` reads five fields to decide what to hash, and
//! whether a given config is coherent is not a question the compiler can ask.

use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;

use crate::freshness;
use crate::verdict::{Refusal, Verdict};

/// The `sha256=` prefix GitHub puts on its digest.
const GITHUB_PREFIX: &str = "sha256=";

/// The `v0=` prefix Slack puts on its digest.
const SLACK_PREFIX: &str = "v0=";

/// The version token Slack's basestring opens with.
///
/// The same `v0` as [`SLACK_PREFIX`] carries, and named separately because they
/// are two different appearances of it: one is a header prefix that gets
/// stripped, the other is signed bytes. A scheme that changed one without the
/// other would be a different scheme.
const SLACK_VERSION: &str = "v0";

/// The field separator in Slack's basestring.
const SLACK_SEPARATOR: &str = ":";

/// How a provider's signature is constructed.
///
/// Every arm signs the RAW request body — the bytes as received, before any
/// JSON parse. A re-serialised body is a different byte string and will not
/// verify, which is why the ingress layer holds the original.
// Deliberately NOT `#[non_exhaustive]`. That attribute hands every other crate
// a `_` arm, which is exactly the escape hatch this enum exists to deny: a
// scheme added without a decision at each call site would compile, and the
// canonicalisation would silently fall through to somebody else's branch.
// The workspace has no external consumers, so the semver freedom it buys is
// freedom nobody needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// A hex digest over the body alone, carried behind `sha256=`.
    ///
    /// GitHub's `x-hub-signature-256`. No timestamp is bound, so this scheme
    /// has no replay window of its own — GitHub's delivery ids are what dedup
    /// rides on instead.
    BodyHex,

    /// A hex digest over the body alone, carried bare.
    ///
    /// Linear's `linear-signature`. Identical to [`Scheme::BodyHex`] but for
    /// the absent prefix, and a separate arm rather than an empty-string prefix
    /// field, because "carries no prefix" is a property of the scheme and an
    /// empty string is a value somebody can paste into a row that needed one.
    BodyHexBare,

    /// A hex digest over `v0:{timestamp}:{body}`, carried behind `v0=`.
    ///
    /// Slack's `x-slack-signature`, paired with `x-slack-request-timestamp`.
    /// This is the only arm here that binds a timestamp, which is why it is the
    /// only one that can answer [`Refusal::StaleTimestamp`].
    SlackV0,
}

impl Scheme {
    /// The header the signature arrives in.
    ///
    /// Lowercase, and that is load-bearing rather than stylistic: header lookup
    /// is a case-insensitive concern the HTTP layer normalises downward, and a
    /// capitalised literal here would match nothing and degrade every delivery
    /// to "no signature presented" — a refusal that reads exactly like an
    /// unsigned request. `trusted_client_ip.zig` carries the same warning about
    /// the same class of bug.
    #[must_use]
    pub const fn signature_header(self) -> &'static str {
        match self {
            Self::BodyHex => "x-hub-signature-256",
            Self::BodyHexBare => "linear-signature",
            Self::SlackV0 => "x-slack-signature",
        }
    }

    /// The header carrying the signed timestamp, for the scheme that binds one.
    ///
    /// `None` is "this scheme has no replay binding", not "unknown".
    #[must_use]
    pub const fn timestamp_header(self) -> Option<&'static str> {
        match self {
            Self::BodyHex | Self::BodyHexBare => None,
            Self::SlackV0 => Some("x-slack-request-timestamp"),
        }
    }

    /// What the header's value is prefixed with, if anything.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::BodyHex => GITHUB_PREFIX,
            Self::BodyHexBare => "",
            Self::SlackV0 => SLACK_PREFIX,
        }
    }

    /// The tag this scheme expects over `body`, given the headers it binds.
    ///
    /// `timestamp` is the header's ORIGINAL bytes — see [`freshness::is_fresh_at`]
    /// for why it is never re-rendered from a parsed integer.
    fn expected_tag(
        self,
        secret: &SecretBytes,
        timestamp: Option<&str>,
        body: &[u8],
    ) -> Option<HmacSha256Tag> {
        match self {
            Self::BodyHex | Self::BodyHexBare => {
                Some(HmacSha256Tag::compute_peppered(secret, &[body]))
            }
            Self::SlackV0 => {
                // A scheme that binds a timestamp cannot sign without one. The
                // caller has already refused a missing header, so this is the
                // unreachable-by-construction arm rather than a second policy.
                let signed_at = timestamp?;
                Some(HmacSha256Tag::compute_peppered(
                    secret,
                    &[
                        SLACK_VERSION.as_bytes(),
                        SLACK_SEPARATOR.as_bytes(),
                        signed_at.as_bytes(),
                        SLACK_SEPARATOR.as_bytes(),
                        body,
                    ],
                ))
            }
        }
    }

    /// Whether `presented` proves `body` was signed with `secret`.
    ///
    /// The whole decision for the hex-digest family, in the order the Zig
    /// decides it: an unusable secret first, then freshness (so a stale
    /// delivery never costs a tag computation), then the prefix, then the
    /// constant-time comparison.
    ///
    /// `now_unix_seconds` is explicit for the reason [`freshness`] gives.
    pub fn verify_at(
        self,
        secret: &SecretBytes,
        presented: Option<&str>,
        timestamp: Option<&str>,
        body: &[u8],
        now_unix_seconds: i64,
    ) -> Verdict {
        // Defence in depth, and not redundant with the caller's own check: an
        // empty key makes the tag deterministic and attacker-computable, so a
        // vault row that came back blank must never reach a comparison. Both
        // `webhook_sig.zig` and `svix_verify.zig` carry this same guard for the
        // same reason.
        if secret.is_empty() {
            return Verdict::Refused(Refusal::Unconfigured);
        }

        let Some(presented) = presented else {
            return Verdict::Refused(Refusal::Signature);
        };

        if self.timestamp_header().is_some() {
            // A scheme that binds a timestamp and did not get one is refused as
            // stale rather than as a bad signature: the header is the replay
            // binding, and its absence is a replay-shaped failure.
            let Some(signed_at) = timestamp else {
                return Verdict::Refused(Refusal::StaleTimestamp);
            };
            if !freshness::is_fresh_at(signed_at, now_unix_seconds, freshness::MAX_DRIFT_SECONDS) {
                return Verdict::Refused(Refusal::StaleTimestamp);
            }
        }

        let Some(digest) = presented.strip_prefix(self.prefix()) else {
            return Verdict::Refused(Refusal::Signature);
        };

        let Some(expected) = self.expected_tag(secret, timestamp, body) else {
            return Verdict::Refused(Refusal::Signature);
        };

        match decode_tag(digest) {
            // `HmacSha256Tag::verify` is the constant-time comparison (RULE
            // CTM); nothing here compares tag bytes with `==`.
            Some(offered) if expected.verify(&offered).is_ok() => Verdict::Verified,
            _ => Verdict::Refused(Refusal::Signature),
        }
    }
}

/// Decodes a lowercase-hex digest into a tag, or `None` if it is not one.
///
/// Length is checked by `from_slice`, so a digest of the wrong width is refused
/// rather than compared against a truncated expectation. `hex::decode` is
/// case-INSENSITIVE where the providers all emit lowercase; accepting both is
/// what the Zig's `hexToBytes` does too, so this is parity rather than laxity.
fn decode_tag(digest: &str) -> Option<HmacSha256Tag> {
    let bytes = hex::decode(digest).ok()?;
    HmacSha256Tag::from_slice(&bytes).ok()
}
