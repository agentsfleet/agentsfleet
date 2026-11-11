//! The answer table's crypto row, which no datastore-free path raises.
#![expect(
    clippy::panic,
    reason = "test: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use af_core::error_code;

use super::{Error, detail};

/// A key that will not parse is the cheapest real `af_crypto` failure; any
/// other one lifts through the same arm.
fn crypto_failure() -> Error {
    match af_crypto::secret::Kek::from_hex("not hex") {
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

/// A statement that failed in flight is the datastore's fault and heals on a
/// second read, though it answers as a 500 and not the outage's 503.
#[test]
fn a_statement_that_failed_in_flight_is_retryable_but_not_an_outage() {
    let dropped = super::query("read the secret")(sqlx::Error::PoolTimedOut);

    assert!(dropped.is_retryable());
    assert!(!dropped.is_datastore_unavailable());
    assert_eq!(dropped.code(), error_code::INTERNAL_DB_QUERY);
}

/// Only the two datastore kinds heal on their own; the rest are the stored
/// bytes' or the caller's.
#[test]
fn only_a_datastore_that_would_not_answer_and_a_failed_statement_are_retryable() {
    assert!(!crypto_failure().is_retryable());
    for refused in [
        super::ErrorKind::NameInvalid,
        super::ErrorKind::DataInvalid,
        super::ErrorKind::DataTooLarge,
        super::ErrorKind::NotFound,
        super::ErrorKind::NameTaken,
        super::ErrorKind::WorkspaceUnknown,
    ] {
        let label = format!("{refused:?}");
        assert!(!Error::from(refused).is_retryable(), "{label}");
    }
}

/// The outage itself is retryable too, built the way the pool raises it.
#[cfg(feature = "test-util")]
#[test]
fn a_datastore_that_would_not_answer_is_retryable_and_an_outage() {
    let Some((_label, unreachable)) = afd_db::error::one_of_each_kind()
        .into_iter()
        .find(|(label, _error)| *label == "datastore unreachable")
    else {
        panic!("the sampler holds a datastore that is unreachable");
    };
    let outage = Error::from(unreachable);

    assert!(outage.is_retryable());
    assert!(outage.is_datastore_unavailable());
}
