//! The vendored Svix verifier, against upstream's own vectors and our patches.
//!
//! Two jobs. The first is upstream's test corpus, carried across from
//! `svix/svix-webhooks` v2.1.0 `rust/src/webhooks.rs`, so a resync can be
//! checked against the behaviour it was copied from. The second is the patch
//! list in `vendor/svix.rs`: each divergence from upstream is asserted HERE in
//! its patched form, with upstream's behaviour named in the test, so a future
//! resync that quietly reverts a patch turns this suite red instead of silently
//! widening what the daemon accepts.
//!
//! The secret and payload below are upstream's published test fixtures. They
//! authenticate nothing.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use afd_webhook::vendor::svix::{SvixHeaders, SvixSecret, verify_at};
use afd_webhook::{Refusal, Verdict};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};

/// Upstream's published test secret. Not a live credential.
const SECRET: &str = "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD";

/// Upstream's published test payload.
const PAYLOAD: &[u8] = br#"{"test": 2432232314}"#;

/// Upstream's published message id.
const MSG_ID: &str = "msg_p5jXN8AQM9LWM0D4loKWxJek";

/// The instant upstream's vectors are signed at.
const NOW: i64 = 1_614_265_330;

fn secret() -> SvixSecret {
    SvixSecret::parse(SECRET).expect("upstream's fixture secret parses")
}

/// Signs exactly as the verifier expects, so a passing test is a round trip
/// rather than a restatement of the implementation's own output.
fn sign(id: &str, timestamp: &str, body: &[u8]) -> String {
    let encoded = SECRET
        .strip_prefix("whsec_")
        .expect("the fixture carries the prefix");
    let raw = STANDARD.decode(encoded).expect("the fixture is base64");
    let tag = HmacSha256Tag::compute_peppered(
        &SecretBytes::new(raw),
        &[id.as_bytes(), b".", timestamp.as_bytes(), b".", body],
    );
    format!("v1,{}", STANDARD.encode(tag.as_bytes()))
}

fn headers<'a>(id: &'a str, timestamp: &'a str, signature: &'a str) -> SvixHeaders<'a> {
    SvixHeaders {
        id,
        timestamp,
        signature,
    }
}

// ── Upstream's corpus ────────────────────────────────────────────────────────

#[test]
fn a_valid_signature_verifies() {
    let ts = NOW.to_string();
    let sig = sign(MSG_ID, &ts, PAYLOAD);
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, &ts, &sig), PAYLOAD, NOW),
        Verdict::Verified
    );
}

#[test]
fn a_tampered_body_does_not_verify() {
    let ts = NOW.to_string();
    let sig = sign(MSG_ID, &ts, PAYLOAD);
    let verdict = verify_at(&secret(), headers(MSG_ID, &ts, &sig), b"{}", NOW);
    assert_eq!(verdict, Verdict::Refused(Refusal::Signature));
}

#[test]
fn a_signature_for_another_message_id_does_not_verify() {
    // The id is signed, so a delivery replayed under a different id fails —
    // which is what makes the id usable for deduplication downstream.
    let ts = NOW.to_string();
    let sig = sign("msg_somethingelse", &ts, PAYLOAD);
    let verdict = verify_at(&secret(), headers(MSG_ID, &ts, &sig), PAYLOAD, NOW);
    assert_eq!(verdict, Verdict::Refused(Refusal::Signature));
}

#[test]
fn one_valid_entry_among_several_verifies() {
    // How Svix rolls a secret without a gap: it signs with both during the
    // overlap, and a receiver accepting either stays up.
    let ts = NOW.to_string();
    let valid = sign(MSG_ID, &ts, PAYLOAD);
    let header = format!("v1,bm90aGluZyB0byBzZWUgaGVyZSBhdCBhbGwsIG5vcGU= {valid}");
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, &ts, &header), PAYLOAD, NOW),
        Verdict::Verified
    );
}

#[test]
fn an_unknown_signature_version_is_skipped_not_refused() {
    // A future `v2` alongside a `v1` must not break a `v1` receiver.
    let ts = NOW.to_string();
    let valid = sign(MSG_ID, &ts, PAYLOAD);
    let header = format!("v2,aGVsbG8gZnJvbSB0aGUgZnV0dXJl {valid}");
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, &ts, &header), PAYLOAD, NOW),
        Verdict::Verified
    );
}

#[test]
fn a_header_of_only_unknown_versions_does_not_verify() {
    let ts = NOW.to_string();
    let header = "v2,aGVsbG8gZnJvbSB0aGUgZnV0dXJl";
    let verdict = verify_at(&secret(), headers(MSG_ID, &ts, header), PAYLOAD, NOW);
    assert_eq!(verdict, Verdict::Refused(Refusal::Signature));
}

#[test]
fn a_partial_or_malformed_entry_does_not_verify() {
    let ts = NOW.to_string();
    let valid = sign(MSG_ID, &ts, PAYLOAD);
    let truncated = &valid[..valid.len() - 4];
    for header in [
        truncated,
        "v1",
        "v1,",
        ",",
        "",
        "v1,!!!not base64!!!",
        "v1,c2hvcnQ=",
    ] {
        let verdict = verify_at(&secret(), headers(MSG_ID, &ts, header), PAYLOAD, NOW);
        assert_eq!(
            verdict,
            Verdict::Refused(Refusal::Signature),
            "`{header}` must not verify"
        );
    }
}

#[test]
fn a_missing_header_does_not_verify() {
    let ts = NOW.to_string();
    let sig = sign(MSG_ID, &ts, PAYLOAD);
    assert_eq!(
        verify_at(&secret(), headers("", &ts, &sig), PAYLOAD, NOW),
        Verdict::Refused(Refusal::Signature)
    );
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, &ts, ""), PAYLOAD, NOW),
        Verdict::Refused(Refusal::Signature)
    );
    // An absent timestamp is refused before the window is consulted, so it
    // reads as a missing proof rather than a stale one.
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, "", &sig), PAYLOAD, NOW),
        Verdict::Refused(Refusal::Signature)
    );
}

#[test]
fn the_tolerance_window_is_five_minutes_in_both_directions() {
    for offset in [-300, 300] {
        let ts = (NOW + offset).to_string();
        let sig = sign(MSG_ID, &ts, PAYLOAD);
        assert_eq!(
            verify_at(&secret(), headers(MSG_ID, &ts, &sig), PAYLOAD, NOW),
            Verdict::Verified,
            "{offset}s is inside the window"
        );
    }
    for offset in [-301, 301] {
        let ts = (NOW + offset).to_string();
        let sig = sign(MSG_ID, &ts, PAYLOAD);
        assert_eq!(
            verify_at(&secret(), headers(MSG_ID, &ts, &sig), PAYLOAD, NOW),
            Verdict::Refused(Refusal::StaleTimestamp),
            "{offset}s is outside the window"
        );
    }
}

#[test]
fn a_stale_delivery_is_refused_as_stale_not_as_a_bad_signature() {
    // The distinction a provider ACTS on: retry a late delivery, never resend a
    // forged one. Signed correctly, so staleness is the only reason to refuse.
    let ts = (NOW - 86_400).to_string();
    let sig = sign(MSG_ID, &ts, PAYLOAD);
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, &ts, &sig), PAYLOAD, NOW),
        Verdict::Refused(Refusal::StaleTimestamp)
    );
}

// ── The local patches (see vendor/svix.rs) ───────────────────────────────────

#[test]
fn patch_1_a_secret_without_the_whsec_prefix_is_refused() {
    // UPSTREAM accepts a bare secret: `strip_prefix(PREFIX).unwrap_or(secret)`.
    // The Zig requires the prefix, and the Zig is the oracle.
    let bare = SECRET
        .strip_prefix("whsec_")
        .expect("the fixture carries the prefix");
    assert!(
        SvixSecret::parse(bare).is_none(),
        "a bare secret must not be accepted — upstream would take it"
    );
}

#[test]
fn patch_3_an_unpadded_secret_is_accepted() {
    // UPSTREAM decodes with padded standard base64 only. The Zig falls back to
    // unpadded, and an operator's already-stored secret must survive cutover.
    let encoded = SECRET
        .strip_prefix("whsec_")
        .expect("the fixture carries the prefix");
    let raw = STANDARD.decode(encoded).expect("the fixture is base64");
    let unpadded = format!("whsec_{}", STANDARD_NO_PAD.encode(&raw));
    assert!(
        SvixSecret::parse(&unpadded).is_some(),
        "an unpadded secret must be accepted — upstream would refuse it"
    );
}

#[test]
fn patch_4_the_timestamp_is_signed_as_its_original_bytes() {
    // UPSTREAM parses the header to an i64 and re-renders it, so `+1700000000`
    // would be signed as `1700000000` — bytes the sender never wrote, letting a
    // respelled timestamp verify. Here the header slice is signed as received,
    // so the two spellings are different messages.
    let plain = NOW.to_string();
    let respelled = format!("+{plain}");

    // `+1700000000` parses to the same instant, so it passes the freshness
    // window exactly as the plain spelling does — the two differ only in the
    // bytes that get signed. Upstream, having re-rendered the parsed integer,
    // would compute the same tag and ACCEPT. Here the basestring carries the
    // header as received, so the tag differs and the delivery is refused.
    let sig_over_plain = sign(MSG_ID, &plain, PAYLOAD);
    assert_eq!(
        verify_at(
            &secret(),
            headers(MSG_ID, &respelled, &sig_over_plain),
            PAYLOAD,
            NOW
        ),
        Verdict::Refused(Refusal::Signature),
        "a respelled timestamp is not the signed one — upstream would accept it"
    );

    // And the round trip over the respelled bytes DOES verify, which is what
    // makes the assertion above about canonicalisation rather than about `+`
    // being rejected somewhere.
    let sig_over_respelled = sign(MSG_ID, &respelled, PAYLOAD);
    assert_eq!(
        verify_at(
            &secret(),
            headers(MSG_ID, &respelled, &sig_over_respelled),
            PAYLOAD,
            NOW
        ),
        Verdict::Verified,
        "the bytes as sent are the bytes that are signed"
    );
}

#[test]
fn patch_5_a_non_utf8_body_is_verified_as_bytes() {
    // UPSTREAM refuses a payload that is not valid UTF-8 before it hashes
    // anything. A signature is over bytes; imposing an encoding is a second
    // thing that can disagree with the daemon beside us.
    let body: &[u8] = &[0xff, 0xfe, 0x00, 0x01];
    let ts = NOW.to_string();
    let sig = sign(MSG_ID, &ts, body);
    assert_eq!(
        verify_at(&secret(), headers(MSG_ID, &ts, &sig), body, NOW),
        Verdict::Verified,
        "a correctly signed binary body verifies — upstream would refuse it"
    );
}

#[test]
fn an_empty_secret_never_parses() {
    // An empty HMAC key makes the tag deterministic and attacker-computable.
    assert!(SvixSecret::parse("whsec_").is_none());
    assert!(SvixSecret::parse("").is_none());
    assert!(SvixSecret::parse("not-base64-at-all!!").is_none());
}
