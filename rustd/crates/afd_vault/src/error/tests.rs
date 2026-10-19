//! The answer table's crypto row, which no datastore-free path raises.
#![expect(
    clippy::panic,
    reason = "test: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::error_code;

use super::{Error, detail};

/// A key that will not parse is the cheapest real `afd_crypto` failure; any
/// other one lifts through the same arm.
fn crypto_failure() -> Error {
    match afd_crypto::secret::Kek::from_hex("not hex") {
        Ok(_unexpected) => panic!("a non-hex key must not parse"),
        Err(source) => Error::from(source),
    }
}

/// A seal or open failure answers the fixed internal sentence: telling the
/// caller which AEAD layer failed would be an oracle on the stored bytes.
#[test]
fn a_crypto_failure_answers_the_fixed_internal_sentence_and_keeps_its_cause() {
    let error = crypto_failure();

    assert_eq!(error.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(error.detail(), detail::OPERATION_FAILED);
    assert!(!error.is_datastore_unavailable());
    assert_eq!(error.referenced_by(), None);
    assert!(
        error.source().is_some(),
        "the operator still gets the crypto cause through the chain"
    );
}
