//! Every scheme against every way a delivery can fail to prove itself.
//!
//! Dimension 1.1's matrix: each scheme × {valid, wrong key, tampered body,
//! missing header, malformed header} lands on the documented verdict. Written
//! as a matrix rather than as three near-identical suites so a scheme added
//! without a row is visible as an absence, and so the verdicts can be read
//! against each other in one place.
//!
//! The secrets here are readable fixtures, not credentials.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use afd_webhook::{Refusal, Scheme, Verdict};

/// A low-entropy fixture: any string round-trips through HMAC, and a
/// high-entropy hex value would trip the secret scanner.
const SECRET: &str = "webhook-test-signing-secret-fixture";

/// A different fixture, for the wrong-key row.
const OTHER_SECRET: &str = "webhook-test-a-completely-other-key";

const BODY: &[u8] = br#"{"event":"deployment_status"}"#;

const NOW: i64 = 1_700_000_000;

/// Every scheme this crate verifies. A new arm added without a row here leaves
/// the matrix visibly short.
const SCHEMES: &[Scheme] = &[Scheme::BodyHex, Scheme::BodyHexBare, Scheme::SlackV0];

fn secret(raw: &str) -> SecretBytes {
    SecretBytes::new(raw.as_bytes().to_vec())
}

/// The timestamp a scheme binds, or `None` when it binds none.
fn timestamp_for(scheme: Scheme, at: i64) -> Option<String> {
    scheme.timestamp_header().map(|_header| at.to_string())
}

/// Signs `body` the way a provider using `scheme` would.
fn sign(scheme: Scheme, key: &str, timestamp: Option<&str>, body: &[u8]) -> String {
    let secret = secret(key);
    let tag = match scheme {
        Scheme::BodyHex | Scheme::BodyHexBare => HmacSha256Tag::compute_peppered(&secret, &[body]),
        Scheme::SlackV0 => {
            let signed_at = timestamp.expect("a timestamped scheme is signed with one");
            HmacSha256Tag::compute_peppered(
                &secret,
                &[b"v0", b":", signed_at.as_bytes(), b":", body],
            )
        }
    };
    format!("{}{}", scheme.prefix(), tag.to_hex())
}

fn verify(
    scheme: Scheme,
    key: &str,
    presented: Option<&str>,
    timestamp: Option<&str>,
    body: &[u8],
) -> Verdict {
    scheme.verify_at(&secret(key), presented, timestamp, body, NOW)
}

#[test]
fn a_correctly_signed_delivery_verifies_on_every_scheme() {
    for &scheme in SCHEMES {
        let ts = timestamp_for(scheme, NOW);
        let sig = sign(scheme, SECRET, ts.as_deref(), BODY);
        assert_eq!(
            verify(scheme, SECRET, Some(&sig), ts.as_deref(), BODY),
            Verdict::Verified,
            "{scheme:?} must accept its own signature"
        );
    }
}

#[test]
fn a_signature_under_the_wrong_key_is_refused_on_every_scheme() {
    for &scheme in SCHEMES {
        let ts = timestamp_for(scheme, NOW);
        let sig = sign(scheme, OTHER_SECRET, ts.as_deref(), BODY);
        assert_eq!(
            verify(scheme, SECRET, Some(&sig), ts.as_deref(), BODY),
            Verdict::Refused(Refusal::Signature),
            "{scheme:?} must refuse a foreign key"
        );
    }
}

#[test]
fn a_tampered_body_is_refused_on_every_scheme() {
    for &scheme in SCHEMES {
        let ts = timestamp_for(scheme, NOW);
        let sig = sign(scheme, SECRET, ts.as_deref(), BODY);
        assert_eq!(
            verify(
                scheme,
                SECRET,
                Some(&sig),
                ts.as_deref(),
                br#"{"event":"x"}"#
            ),
            Verdict::Refused(Refusal::Signature),
            "{scheme:?} must refuse a body it did not sign"
        );
    }
}

#[test]
fn an_absent_signature_header_is_refused_on_every_scheme() {
    for &scheme in SCHEMES {
        let ts = timestamp_for(scheme, NOW);
        assert_eq!(
            verify(scheme, SECRET, None, ts.as_deref(), BODY),
            Verdict::Refused(Refusal::Signature),
            "{scheme:?} must refuse a delivery presenting nothing"
        );
    }
}

#[test]
fn a_malformed_signature_header_is_refused_on_every_scheme() {
    for &scheme in SCHEMES {
        let ts = timestamp_for(scheme, NOW);
        let valid = sign(scheme, SECRET, ts.as_deref(), BODY);
        let malformed = [
            String::new(),
            "not-hex-at-all".to_owned(),
            // The digest without its scheme prefix.
            valid
                .strip_prefix(scheme.prefix())
                .unwrap_or(&valid)
                .to_owned(),
            // A prefix with nothing behind it.
            scheme.prefix().to_owned(),
            // Correct shape, truncated: refused on width, never compared
            // against a shortened expectation.
            valid[..valid.len() - 2].to_owned(),
            // Correct width, wrong alphabet.
            format!("{}{}", scheme.prefix(), "z".repeat(64)),
        ];
        for candidate in malformed {
            // A bare-digest scheme has no prefix to strip, so the "digest
            // without prefix" row IS the valid signature there. Skip only that
            // exact coincidence rather than weakening the row for every scheme.
            if candidate == valid {
                continue;
            }
            assert_eq!(
                verify(scheme, SECRET, Some(&candidate), ts.as_deref(), BODY),
                Verdict::Refused(Refusal::Signature),
                "{scheme:?} must refuse `{candidate}`"
            );
        }
    }
}

#[test]
fn an_empty_secret_is_refused_as_unconfigured_before_any_comparison() {
    // Dimension 1.4. An empty HMAC key is deterministic and attacker-
    // computable, so a blank vault row must never reach a comparison — and it
    // is reported as a misconfiguration rather than as a bad signature, because
    // the remedy is the operator's, not the sender's.
    for &scheme in SCHEMES {
        let ts = timestamp_for(scheme, NOW);
        let sig = sign(scheme, SECRET, ts.as_deref(), BODY);
        assert_eq!(
            verify(scheme, "", Some(&sig), ts.as_deref(), BODY),
            Verdict::Refused(Refusal::Unconfigured),
            "{scheme:?} must refuse an unconfigured secret as such"
        );
    }
}

#[test]
fn only_the_timestamped_scheme_binds_a_window() {
    // Dimension 1.2, and the reason the schemes are separate arms: a scheme
    // with no timestamp header cannot answer StaleTimestamp at all, and one
    // with a timestamp header must answer it at the documented edge.
    for &scheme in SCHEMES {
        let Some(_header) = scheme.timestamp_header() else {
            // No binding: a delivery signed long ago still verifies, because
            // nothing in the scheme says when it was signed.
            let sig = sign(scheme, SECRET, None, BODY);
            assert_eq!(
                verify(scheme, SECRET, Some(&sig), None, BODY),
                Verdict::Verified,
                "{scheme:?} binds no timestamp and so cannot be stale"
            );
            continue;
        };

        let inside = (NOW - 299).to_string();
        let sig_inside = sign(scheme, SECRET, Some(&inside), BODY);
        assert_eq!(
            verify(scheme, SECRET, Some(&sig_inside), Some(&inside), BODY),
            Verdict::Verified,
            "{scheme:?} accepts 4m59s"
        );

        let outside = (NOW - 301).to_string();
        let sig_outside = sign(scheme, SECRET, Some(&outside), BODY);
        assert_eq!(
            verify(scheme, SECRET, Some(&sig_outside), Some(&outside), BODY),
            Verdict::Refused(Refusal::StaleTimestamp),
            "{scheme:?} refuses 5m01s as stale"
        );
    }
}

#[test]
fn a_timestamped_scheme_missing_its_timestamp_is_refused_as_stale() {
    // The header IS the replay binding, so its absence is a replay-shaped
    // failure rather than a malformed signature.
    let ts = NOW.to_string();
    let sig = sign(Scheme::SlackV0, SECRET, Some(&ts), BODY);
    assert_eq!(
        verify(Scheme::SlackV0, SECRET, Some(&sig), None, BODY),
        Verdict::Refused(Refusal::StaleTimestamp)
    );
}

#[test]
fn every_scheme_has_a_distinct_lowercase_signature_header() {
    // Two schemes sharing a header would make detection by header ambiguous,
    // and a capitalised literal would match nothing after the HTTP layer
    // normalises — degrading every delivery to "no signature presented", a
    // refusal indistinguishable from an unsigned request.
    for (index, &scheme) in SCHEMES.iter().enumerate() {
        let header = scheme.signature_header();
        assert!(!header.is_empty(), "{scheme:?} needs a signature header");
        assert_eq!(
            header,
            header.to_ascii_lowercase(),
            "{scheme:?}'s header must be lowercase"
        );
        for &other in SCHEMES.iter().skip(index + 1) {
            assert_ne!(
                header,
                other.signature_header(),
                "{scheme:?} and {other:?} share a header"
            );
        }
    }
}

#[test]
fn a_refusal_carries_its_registry_code_and_a_stable_sentence() {
    // Invariant 5's other half: what a rejection is allowed to say. The code
    // and the sentence are the whole public surface — no payload, no signature.
    assert_eq!(
        Refusal::Unconfigured.code().as_str(),
        "UZ-WH-020",
        "an unconfigured secret is a misconfiguration, not a bad signature"
    );
    assert_eq!(Refusal::Signature.code().as_str(), "UZ-WH-010");
    assert_eq!(Refusal::StaleTimestamp.code().as_str(), "UZ-WH-011");

    // Byte-identical to the Zig daemon's, which a provider's delivery log shows
    // to an operator debugging their integration.
    assert_eq!(
        Refusal::Unconfigured.detail(),
        "Webhook credential not configured"
    );
    assert_eq!(Refusal::Signature.detail(), "Invalid signature");
    assert_eq!(
        Refusal::StaleTimestamp.detail(),
        "Signature timestamp too old"
    );
}

#[test]
fn a_verdict_reports_its_refusal_and_nothing_else() {
    assert!(Verdict::Verified.is_verified());
    assert_eq!(Verdict::Verified.refusal(), None);
    let refused = Verdict::Refused(Refusal::StaleTimestamp);
    assert!(!refused.is_verified());
    assert_eq!(refused.refusal(), Some(Refusal::StaleTimestamp));
}
