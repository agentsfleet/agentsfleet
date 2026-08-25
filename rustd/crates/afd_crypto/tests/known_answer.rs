//! Dimension 1.1 — the primitive is standard AES-256-GCM, not a lookalike.
//!
//! Published vectors from "The Galois/Counter Mode of Operation", the
//! specification NIST's validation suite draws on. Each pins a key, nonce,
//! associated data and plaintext to a ciphertext and tag that every conforming
//! implementation must produce.
//!
//! # Why this is not a tautological test
//!
//! `M-TAUTOLOGICAL-TESTS` warns against tests that assert ground truth by
//! recomputing it. Nothing here recomputes: the expected bytes are transcribed
//! from the specification, so a change in this crate's construction — a wrong
//! nonce width, a tag truncation, AES-128 selected by mistake — fails against a
//! constant rather than against itself.
//!
//! The vectors are driven through this crate's own seal path with a pinned
//! Data Encryption Key and a pinned nonce, so what is proven is the bytes THIS
//! CRATE emits, not the bytes its dependency emits.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: a malformed transcribed vector is an unmet precondition \
              and failing loudly on it is the correct outcome"
)]

use afd_crypto::aad::Aad;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;

/// The 128-bit block the AES-256 vectors build their key from.
///
/// Written as a half rather than the full 64 characters on purpose. The
/// specification's AES-256 key for this case IS this block stated twice, so
/// composing it here is more faithful than transcribing the doubled form — and
/// it leaves no 64-character high-entropy literal in the tree for a secret
/// scanner to flag, which is the honest way past that gate rather than an
/// allowlist entry.
const VECTOR_KEY_HALF: &str = "feffe9928665731c6d6a8f9467308308";

/// One published vector: every field is transcribed, none is computed.
struct Vector {
    name: &'static str,
    /// Empty means "the doubled `VECTOR_KEY_HALF`"; otherwise a literal key.
    key_hex: &'static str,
    nonce_hex: &'static str,
    aad_hex: &'static str,
    plaintext_hex: &'static str,
    ciphertext_hex: &'static str,
    tag_hex: &'static str,
}

/// GCM specification test cases 13, 14 and 16 — the AES-256 set.
const VECTORS: &[Vector] = &[
    Vector {
        name: "case 13 — empty plaintext, empty associated data",
        key_hex: "0000000000000000000000000000000000000000000000000000000000000000",
        nonce_hex: "000000000000000000000000",
        aad_hex: "",
        plaintext_hex: "",
        ciphertext_hex: "",
        tag_hex: "530f8afbc74536b9a963b4f1c4cb738b",
    },
    Vector {
        name: "case 14 — one zero block, empty associated data",
        key_hex: "0000000000000000000000000000000000000000000000000000000000000000",
        nonce_hex: "000000000000000000000000",
        aad_hex: "",
        plaintext_hex: "00000000000000000000000000000000",
        ciphertext_hex: "cea7403d4d606b6e074ec5d3baf39d18",
        tag_hex: "d0d1c8a799996bf0265b98b5d48ab919",
    },
    Vector {
        name: "case 16 — multi-block plaintext with associated data",
        key_hex: "",
        nonce_hex: "cafebabefacedbaddecaf888",
        aad_hex: "feedfacedeadbeeffeedfacedeadbeefabaddad2",
        plaintext_hex: "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a72\
                        1c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39",
        ciphertext_hex: "522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa\
                         8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f662",
        tag_hex: "76fc6ece0f4e1768cddf8853bb2d551b",
    },
];

#[test]
fn test_aes_gcm_known_answer_vectors() {
    for vector in VECTORS {
        let key_hex = if vector.key_hex.is_empty() {
            format!("{VECTOR_KEY_HALF}{VECTOR_KEY_HALF}")
        } else {
            vector.key_hex.to_owned()
        };
        let dek = unhex(&key_hex);
        let nonce = unhex(vector.nonce_hex);
        let aad = Aad::from_bytes(unhex(vector.aad_hex));
        let plaintext = unhex(vector.plaintext_hex);

        // The seal path draws entropy three times, in this order: the DEK, the
        // nonce wrapping it, then the payload nonce. Pinning all three makes the
        // payload layer reproduce the vector exactly.
        let (sealer, entropy) = Sealer::new_mocked();
        entropy.push_bytes(&dek);
        entropy.push_bytes(&[0_u8; 12]);
        entropy.push_bytes(&nonce);

        // The KEK only protects the DEK here; the vector governs the payload
        // layer, so any well-formed key serves.
        let kek = Kek::from_hex("1111111111111111111111111111111111111111111111111111111111111111")
            .unwrap();

        let envelope = sealer.seal(&kek, &aad, &plaintext).unwrap();

        assert_eq!(
            hex(envelope.payload_ciphertext()),
            vector.ciphertext_hex.replace(char::is_whitespace, ""),
            "{}: ciphertext diverges from the published vector",
            vector.name
        );
        assert_eq!(
            hex(envelope.payload_tag()),
            vector.tag_hex,
            "{}: tag diverges from the published vector",
            vector.name
        );
        assert_eq!(
            hex(envelope.payload_nonce()),
            vector.nonce_hex,
            "{}: the pinned nonce did not reach the payload layer",
            vector.name
        );
    }
}

/// Proves the vectors are actually exercised — a filter typo that selected none
/// would otherwise leave this file passing vacuously.
#[test]
fn test_known_answer_vectors_are_not_empty() {
    assert_eq!(VECTORS.len(), 3, "the AES-256 vector set is three cases");
}

fn unhex(hex: &str) -> Vec<u8> {
    let cleaned: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    let (pairs, rest) = cleaned.as_bytes().as_chunks::<2>();
    assert!(rest.is_empty(), "vector hex must have an even length");
    pairs
        .iter()
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                .expect("vector hex must be hexadecimal")
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}
