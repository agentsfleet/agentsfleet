//! The error type's own surface: every accessor, code, rendering and source.
//!
//! These paths are the ones a human reads at three in the morning, and they are
//! the easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or a `source()` that returns the wrong link, only
//! shows up while something else is already going wrong.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_crypto::aad::Aad;
use afd_crypto::envelope::{Envelope, Sealer};
use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::{Dek, Kek};

const KEK_HEX: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

fn kek() -> Kek {
    Kek::from_hex(KEK_HEX).unwrap()
}

/// One error of each kind, so every accessor is exercised against every kind.
fn one_of_each() -> Vec<(&'static str, afd_crypto::error::Error)> {
    let aad = Aad::new("ws_1", "k");
    let envelope = Sealer::new().seal(&kek(), &aad, b"payload").unwrap();

    let (sealer, entropy) = Sealer::new_mocked();
    entropy.fail_next();

    vec![
        ("key hex", Kek::from_hex("nope").expect_err("short hex")),
        (
            "malformed envelope",
            Dek::from_slice(&[0_u8; 3]).expect_err("3 bytes is not a key"),
        ),
        (
            "open failed",
            envelope
                .open(&kek(), &Aad::new("ws_9", "k"))
                .expect_err("foreign associated data"),
        ),
        (
            "mac mismatch",
            HmacSha256Tag::compute(&kek(), b"a")
                .verify(&HmacSha256Tag::compute(&kek(), b"b"))
                .expect_err("different messages"),
        ),
        (
            "entropy",
            sealer.seal(&kek(), &aad, b"x").expect_err("dead entropy"),
        ),
    ]
}

/// Exactly one accessor answers true for each error — no overlap, no gaps.
#[test]
fn test_every_error_answers_exactly_one_accessor() {
    for (label, error) in one_of_each() {
        let answers = [
            error.is_key_hex(),
            error.is_malformed_envelope(),
            error.is_open_failed(),
            error.is_mac_mismatch(),
            error.is_entropy(),
        ];
        assert_eq!(
            answers.iter().filter(|answer| **answer).count(),
            1,
            "{label}: expected exactly one accessor to answer true, got {answers:?}"
        );
    }
}

/// Each kind maps to the registry code the Zig daemon reports for it.
#[test]
fn test_error_codes_match_the_registry() {
    for (label, error) in one_of_each() {
        let expected = if error.is_malformed_envelope() {
            "UZ-VAULT-001"
        } else {
            "UZ-INTERNAL-003"
        };
        assert_eq!(error.code().as_str(), expected, "{label} mapped wrongly");
    }
}

/// The rendering leads with the code and then says what happened.
#[test]
fn test_error_display_leads_with_the_code() {
    for (label, error) in one_of_each() {
        let rendered = error.to_string();
        assert!(
            rendered.starts_with(&format!("[{}]", error.code().as_str())),
            "{label}: expected a leading code, got {rendered}"
        );
        assert!(
            rendered.len() > error.code().as_str().len() + 2,
            "{label}: expected a description after the code, got {rendered}"
        );
    }
}

/// No rendering carries key material, including the one that formats a length.
#[test]
fn test_error_rendering_carries_no_key_material() {
    for (label, error) in one_of_each() {
        let rendered = format!("{error}{error:?}");
        assert!(!rendered.contains(KEK_HEX), "{label} leaked the key");
        assert!(!rendered.contains("payload"), "{label} leaked a plaintext");
    }
}

/// Every error in this crate is a ROOT, and says everything in its own message.
///
/// None of the kinds wrap a foreign error, and one of them declines to on
/// purpose: `EnvelopeOpen` does not carry the AEAD library's reason, because
/// distinguishing "bad tag" from "bad nonce" for a caller is the beginning of a
/// padding oracle. So a chain walker must find nothing beneath these.
///
/// This asserted `is_some()` until the source of every error in the workspace
/// was its own private `ErrorKind` — which made `{:#}` print each message
/// twice and published a `pub(crate)` type through a public trait.
#[test]
fn test_every_error_is_a_root_with_a_complete_message() {
    for (label, error) in one_of_each() {
        assert!(
            error.source().is_none(),
            "{label} reports a cause; nothing in this crate wraps one"
        );
        assert!(
            !error.to_string().is_empty(),
            "{label} must carry its whole explanation itself"
        );
    }
}

/// The backtrace is reachable and, by default, not captured.
///
/// `Backtrace::capture` is opt-in via `RUST_BACKTRACE`, so the common path costs
/// a few instructions. Asserting the accessor works without asserting it is
/// populated keeps this test independent of the environment it runs in.
#[test]
fn test_error_exposes_its_backtrace() {
    let error = Kek::from_hex("nope").expect_err("short hex");
    let status = error.backtrace().status();
    assert!(
        matches!(
            status,
            std::backtrace::BacktraceStatus::Captured
                | std::backtrace::BacktraceStatus::Disabled
                | std::backtrace::BacktraceStatus::Unsupported
        ),
        "unexpected backtrace status {status:?}"
    );
}

/// A well-formed length carrying a character that is not a hex digit.
///
/// The gap this closes: every prior hex test was the WRONG LENGTH, so the
/// digit check had no coverage at all and a decoder that accepted anything of
/// the right size would have passed the suite.
#[test]
fn test_a_bad_digit_at_the_right_length_is_refused() {
    for spoiled in ["zz", "g0", "0 ", "0-", "é\u{301}"] {
        let mut text = KEK_HEX.to_owned();
        text.replace_range(0..spoiled.len().min(2), &spoiled[..spoiled.len().min(2)]);
        let error = Kek::from_hex(&text).expect_err("a non-hex digit must be refused");

        assert!(
            error.is_key_hex(),
            "{spoiled:?} was not reported as key hex"
        );
    }
}

/// Uppercase hex names the same key as lowercase.
///
/// Deliberate, and worth pinning: an operator pasting a key from a tool that
/// renders uppercase must not get a daemon that refuses to boot. The identifier
/// spelling rules elsewhere in this workspace reject uppercase; key material
/// does not, because it has no canonical stored form to keep unique.
#[test]
fn test_uppercase_hex_names_the_same_key() {
    let aad = Aad::new("ws_1", "k");
    let lower = Kek::from_hex(KEK_HEX).unwrap();
    let upper = Kek::from_hex(&KEK_HEX.to_uppercase()).expect("uppercase hex is accepted");

    let envelope = Sealer::new().seal(&lower, &aad, b"payload").unwrap();
    assert_eq!(
        envelope.open(&upper, &aad).unwrap().expose(),
        b"payload",
        "the two spellings must name one key"
    );
}

/// The unused-in-production constructors still work, and agree with the others.
#[test]
fn test_key_constructors_agree() {
    let aad = Aad::new("ws_1", "k");
    let from_hex = Kek::from_hex(KEK_HEX).unwrap();
    let from_bytes = Kek::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ]);
    let envelope = Sealer::new().seal(&from_hex, &aad, b"payload").unwrap();
    assert_eq!(
        envelope.open(&from_bytes, &aad).unwrap().expose(),
        b"payload",
        "from_hex and from_bytes must produce the same key"
    );
}

/// The default sealer is the native one, so `Default` is not a second policy.
#[test]
fn test_default_sealer_seals() {
    let aad = Aad::new("ws_1", "k");
    let envelope = Sealer::default().seal(&kek(), &aad, b"payload").unwrap();
    assert_eq!(envelope.open(&kek(), &aad).unwrap().expose(), b"payload");
}

/// A rebuilt envelope equals the original, which `PartialEq` promises callers.
#[test]
fn test_envelope_equality_is_component_wise() {
    let aad = Aad::new("ws_1", "k");
    let original = Sealer::new().seal(&kek(), &aad, b"payload").unwrap();
    let rebuilt = Envelope::from_parts(
        original.wrapped_dek().to_vec(),
        original.dek_nonce(),
        original.dek_tag(),
        original.payload_nonce(),
        original.payload_ciphertext().to_vec(),
        original.payload_tag(),
        original.kek_version(),
    )
    .unwrap();
    assert_eq!(original, rebuilt);
}

/// The rendering appends the backtrace when one was actually captured.
///
/// `Backtrace::capture()` is opt-in via `RUST_BACKTRACE`, and std decides once
/// per process, so this branch cannot be reached from a test sharing a process
/// with the others. Re-running THIS test in a child with the variable set is
/// what makes it deterministic rather than dependent on how the suite was
/// invoked — and the child is a normal instrumented run, so its coverage counts.
#[test]
fn test_error_display_appends_a_captured_backtrace() {
    const CHILD_MARKER: &str = "AFD_CRYPTO_BACKTRACE_CHILD";

    if std::env::var_os(CHILD_MARKER).is_some() {
        let error = Kek::from_hex("nope").expect_err("short hex");
        assert_eq!(
            error.backtrace().status(),
            std::backtrace::BacktraceStatus::Captured,
            "the child sets RUST_BACKTRACE, so capture must have happened"
        );
        let rendered = error.to_string();
        assert!(
            rendered.lines().count() > 1,
            "a captured backtrace must be appended, got {rendered}"
        );
        assert!(
            rendered.starts_with("[UZ-INTERNAL-003]"),
            "the code still leads, got {rendered}"
        );
        return;
    }

    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let output = std::process::Command::new(exe)
        .args([
            "test_error_display_appends_a_captured_backtrace",
            "--exact",
            "--nocapture",
        ])
        .env("RUST_BACKTRACE", "1")
        .env(CHILD_MARKER, "1")
        .output()
        .expect("the child test process starts");

    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
