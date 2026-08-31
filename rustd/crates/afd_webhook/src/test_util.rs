//! Presenting a signature this crate would accept.
//!
//! # Why a signer lives here rather than in each test
//!
//! Verification is this crate's job and signing is its exact inverse, but only
//! verification is a production capability — nothing this daemon runs ever signs
//! a webhook, because it is always the receiver. So the signer is real code with
//! no production caller, which is precisely what a `test-util` feature is for
//! (the `afd_redis::test_util` precedent).
//!
//! The alternative was letting each test compose the signed bytes itself. For
//! [`Scheme::BodyHex`] that is harmless — the bytes are the body. For
//! [`Scheme::SlackV0`] it is not: the signed material is `v0`, a separator, the
//! timestamp, the separator again and then the body, and all four parts are
//! private to `scheme`. A test that re-spelled them would agree with the
//! verifier today and keep passing on the day the composition changed, which is
//! the single day the test exists for.

use afd_crypto::secret::SecretBytes;

use crate::scheme::Scheme;

/// The signature header value proving `body` was signed with `secret`.
///
/// `timestamp` is required by exactly the schemes that bind one — pass the
/// header's original bytes, not a re-rendered integer, for the reason
/// [`crate::freshness::is_fresh_at`] gives.
///
/// `None` when a scheme that binds a timestamp was given none — a caller's own
/// bug rather than a runtime condition, since the verifier refuses a missing
/// timestamp HEADER long before it composes anything. It is returned rather
/// than panicked because this compiles as library code, where the workspace
/// denies `expect`; a test unwraps it under its own allowance and gets to say
/// which case it meant.
#[must_use]
pub fn signature(
    scheme: Scheme,
    secret: &[u8],
    timestamp: Option<&str>,
    body: &[u8],
) -> Option<String> {
    let tag = scheme.expected_tag(&SecretBytes::new(secret.to_vec()), timestamp, body)?;
    Some(format!("{}{}", scheme.prefix(), tag.to_hex()))
}
