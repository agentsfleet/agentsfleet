//! Svix v1 webhook signature verification.
//!
//! ```text
//! DERIVED FROM   svix/svix-webhooks
//! UPSTREAM PATH  rust/src/webhooks.rs
//! RELEASE        v2.1.0
//! COMMIT         da9b423d505f27206668db219de72cb6d69f894a
//! SYNCHRONISED   2026-08-29
//! LICENCE        MIT
//! ```
//!
//! # Why this is vendored rather than depended on
//!
//! The `svix` crate exposes no verifier-only build. `default-features = false`
//! does not compile — its `connector` module is not gated behind the TLS
//! features — and the default build brings the whole API client, an HTTP stack
//! and a connector graph along for a signature check. Measured, not assumed.
//!
//! Vendoring rather than reimplementing is the other half of that call. Svix has
//! already shipped a fix for a bug where certain signatures could bypass
//! verification; a from-the-docs reimplementation inherits today's understanding
//! of the protocol and none of tomorrow's fixes. A vendored file with a commit
//! on it can be DIFFED when upstream moves.
//!
//! # LOCAL PATCHES — read these before diffing upstream
//!
//! The Zig daemon is the behavioural oracle for this milestone, not svix 2.1.0.
//! It serves production and is the rollback target, so where upstream and
//! `auth/crypto/svix_verify.zig` disagree, the Zig wins and the divergence is
//! listed here rather than silently absorbed.
//!
//! 1. **`whsec_` is REQUIRED.** Upstream strips it when present and accepts a
//!    bare secret otherwise (`strip_prefix(PREFIX).unwrap_or(secret)`); the Zig
//!    refuses a secret without it. Accepting both would widen what this daemon
//!    takes as a signing key relative to the daemon beside it.
//! 2. **Only `svix-*` headers.** Upstream also reads the unbranded
//!    `webhook-id` / `webhook-timestamp` / `webhook-signature` spellings. The
//!    Zig reads neither, and accepting them here would let a delivery verify
//!    against this daemon that the other refuses.
//! 3. **Unpadded base64 secrets are accepted.** Upstream decodes with
//!    `BASE64_STANDARD` only; the Zig tries padded, then unpadded. This is the
//!    one place the Zig is LAXER and the port follows it, because a secret an
//!    operator has already stored must keep working across the cutover.
//! 4. **The timestamp is signed as its ORIGINAL bytes.** Upstream parses the
//!    header to an `i64` and re-renders it into the basestring, so a spelling
//!    like `+1700000000` would be signed as `1700000000` — bytes the sender
//!    never wrote. The Zig signs the header slice as received. This one is a
//!    correctness fix, not merely a parity choice.
//! 5. **The payload is raw bytes.** Upstream requires valid UTF-8
//!    (`std::str::from_utf8`) and refuses otherwise; the Zig hashes the body as
//!    received. A signature is over bytes, and imposing an encoding on them is
//!    a second thing that can disagree.
//! 6. **Comparison is over DECODED tags.** Upstream compares base64 STRINGS in
//!    constant time; this decodes each candidate to 32 bytes and compares
//!    through [`HmacSha256Tag::verify`], which is the workspace's one
//!    constant-time primitive (RULE CTM/OWN). Equivalent in security, and it
//!    keeps every tag comparison in this binary on one implementation.
//! 7. **`hmac-sha256` is replaced by `afd_crypto`.** One HMAC in the binary.
//!
//! Upstream's own test vectors ride along in `tests/svix_vendor.rs`, and the
//! cases these patches change are asserted there in their PATCHED form with the
//! upstream behaviour named — so a resync that quietly reverts a patch fails.

use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};

use crate::freshness;
use crate::verdict::{Refusal, Verdict};

/// The prefix a Svix signing secret carries. PATCH 1: required, not optional.
const SECRET_PREFIX: &str = "whsec_";

/// The signature version this verifier accepts. Entries at other versions are
/// skipped rather than refused, which is how upstream leaves room for a `v2`.
const SIGNATURE_VERSION: &str = "v1";

/// The separator between version and digest inside one signature entry.
const VERSION_SEPARATOR: char = ',';

/// The separator between signature entries in the header.
const ENTRY_SEPARATOR: char = ' ';

/// The separator between the three signed fields.
const FIELD_SEPARATOR: &str = ".";

/// The header carrying the message id. PATCH 2: no unbranded alias.
pub const ID_HEADER: &str = "svix-id";

/// The header carrying the signed timestamp. PATCH 2: no unbranded alias.
pub const TIMESTAMP_HEADER: &str = "svix-timestamp";

/// The header carrying the signature list. PATCH 2: no unbranded alias.
pub const SIGNATURE_HEADER: &str = "svix-signature";

/// A Svix signing secret, decoded and ready to verify with.
///
/// Holds the decoded key rather than the `whsec_` string, so the prefix rule and
/// the base64 decode happen once at construction instead of per delivery.
#[derive(Debug)]
pub struct SvixSecret(SecretBytes);

impl SvixSecret {
    /// Decodes a `whsec_`-prefixed secret.
    ///
    /// `None` when the prefix is absent (PATCH 1), the body is not base64
    /// (PATCH 3 accepts padded and unpadded), or the decoded key is empty — an
    /// empty HMAC key is attacker-computable, which is the same defence
    /// `verifySvix` and `webhook_sig.zig` both carry.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let encoded = raw.strip_prefix(SECRET_PREFIX)?;
        let decoded = STANDARD
            .decode(encoded)
            .or_else(|_padded| STANDARD_NO_PAD.decode(encoded))
            .ok()?;
        if decoded.is_empty() {
            return None;
        }
        Some(Self(SecretBytes::new(decoded)))
    }
}

/// The three headers a Svix delivery presents.
///
/// A struct rather than three parameters so a call site cannot transpose the id
/// and the timestamp — they are both opaque strings, and the compiler would not
/// notice.
#[derive(Debug, Clone, Copy)]
pub struct SvixHeaders<'delivery> {
    /// `svix-id` — the message id, signed as the first field.
    pub id: &'delivery str,
    /// `svix-timestamp` — unix seconds, signed as its original bytes (PATCH 4).
    pub timestamp: &'delivery str,
    /// `svix-signature` — space-separated `v1,<base64>` entries.
    pub signature: &'delivery str,
}

/// Whether a Svix delivery proves itself.
///
/// Decision order is upstream's, which is also the Zig's: headers present,
/// timestamp fresh, then the tag. Freshness precedes the tag so a replayed
/// delivery costs no HMAC.
///
/// `now_unix_seconds` is explicit for the reason [`freshness`] gives.
pub fn verify_at(
    secret: &SvixSecret,
    headers: SvixHeaders<'_>,
    body: &[u8],
    now_unix_seconds: i64,
) -> Verdict {
    if headers.id.is_empty() || headers.timestamp.is_empty() || headers.signature.is_empty() {
        return Verdict::Refused(Refusal::Signature);
    }

    if !freshness::is_fresh_at(
        headers.timestamp,
        now_unix_seconds,
        freshness::MAX_DRIFT_SECONDS,
    ) {
        return Verdict::Refused(Refusal::StaleTimestamp);
    }

    // PATCH 4 + 5: the timestamp's original bytes, and the body as bytes.
    let expected = HmacSha256Tag::compute_peppered(
        &secret.0,
        &[
            headers.id.as_bytes(),
            FIELD_SEPARATOR.as_bytes(),
            headers.timestamp.as_bytes(),
            FIELD_SEPARATOR.as_bytes(),
            body,
        ],
    );

    if any_entry_matches(headers.signature, &expected) {
        Verdict::Verified
    } else {
        Verdict::Refused(Refusal::Signature)
    }
}

/// Whether any `v1` entry in the header matches `expected`.
///
/// Multiple signatures is how Svix rotates a secret without a gap — during a
/// roll it signs with both, and a receiver that accepts either stays up. Entries
/// at an unknown version are SKIPPED rather than refused, so a future `v2`
/// alongside a `v1` still verifies here.
///
/// PATCH 6: each candidate is decoded to a tag and compared through the
/// workspace's constant-time primitive, where upstream compares base64 text.
fn any_entry_matches(header: &str, expected: &HmacSha256Tag) -> bool {
    header
        .split(ENTRY_SEPARATOR)
        .filter_map(|entry| entry.split_once(VERSION_SEPARATOR))
        .filter(|(version, _digest)| *version == SIGNATURE_VERSION)
        .filter_map(|(_version, digest)| decode_tag(digest))
        // `any` short-circuits on the first MATCH, which leaks nothing: the
        // number of entries is attacker-supplied but the per-entry comparison
        // is constant-time, and stopping early on success reveals only that a
        // valid signature was found — which the response says anyway.
        .any(|offered| expected.verify(&offered).is_ok())
}

/// Decodes one base64 digest into a tag, or `None` if it is not one.
///
/// Both padded and unpadded, for the same reason the secret accepts both.
/// A digest of the wrong width is refused by `from_slice` rather than compared
/// against a truncated expectation.
fn decode_tag(digest: &str) -> Option<HmacSha256Tag> {
    let bytes = STANDARD
        .decode(digest)
        .or_else(|_padded| STANDARD_NO_PAD.decode(digest))
        .ok()?;
    HmacSha256Tag::from_slice(&bytes).ok()
}
