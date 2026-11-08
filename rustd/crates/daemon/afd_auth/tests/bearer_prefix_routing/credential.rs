//! The credential itself: what it renders as, and the digest it hashes to.
//!
//! Split from the parent for length; the fixtures every class shares stay
//! there.

use afd_auth::credential::{CLI_CREDENTIAL_PREFIX, Presented};
use afd_auth::directory::Digest;

use super::{BODY, present};

// ── The credential itself ────────────────────────────────────────────────

/// A presented credential never renders, whatever holds it.
///
/// A bearer value in a log IS the credential, and a derived `Debug` on any
/// struct that transitively holds one is how it gets there.
#[test]
fn test_a_presented_credential_never_renders_its_value() {
    let presented = present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}"));
    let rendered = format!("{presented:?}");
    assert!(!rendered.contains(BODY), "{rendered}");
    assert!(rendered.contains("redacted"), "{rendered}");
    assert_eq!(presented.len(), CLI_CREDENTIAL_PREFIX.len() + BODY.len());
    assert!(!presented.is_empty());
    // pin test: literal is the contract
    assert_eq!(
        format!("{:?}", present("abc")),
        "Presented(3 bytes, redacted)"
    );
}

/// The digest is the plain SHA-256 a credential column stores, byte for byte.
///
/// Pinned against the published test vector: a digest computed any other way
/// matches no stored row, and nothing else would fail.
#[test]
fn test_a_minted_digest_is_the_published_sha256_of_its_bytes() {
    // pin test: literal is the contract
    assert_eq!(
        Digest::of_minted("abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(Digest::of(&present("abc")), Digest::of_minted("abc"));
}

/// The header parse is exact: the scheme is case-sensitive and the value is
/// kept untrimmed, so a credential hashes to the bytes the client sent.
#[test]
fn test_the_header_parse_is_case_sensitive_and_untrimmed() {
    assert_eq!(
        Presented::from_authorization("Bearer abc")
            .expect("a token")
            .expose(),
        "abc"
    );
    // Untrimmed: the stored digest is over the raw value, so trimming here
    // would hash different bytes than the column holds.
    assert_eq!(
        Presented::from_authorization("Bearer  abc ")
            .expect("a token")
            .expose(),
        " abc "
    );
    for bad in ["bearer abc", "BEARER abc", "Bearer", "Bearer   ", "abc", ""] {
        assert!(Presented::from_authorization(bad).is_err(), "{bad:?}");
    }
}
