//! Gate-owned refusal and query classifications.

use std::error::Error as _;

use afd_core::error_code;

use super::{Error, query, rejected};

#[test]
fn a_caller_refusal_preserves_its_sentence_and_is_not_an_outage() {
    let failure = rejected("approval mode is not supported");

    assert_eq!(failure.code(), error_code::INVALID_REQUEST);
    assert_eq!(failure.detail(), "approval mode is not supported");
    assert!(failure.is_rejected());
    assert!(!failure.is_datastore_unavailable());
    assert!(!failure.is_config_permanent());
    assert!(failure.source().is_none());
}

#[test]
fn a_query_failure_keeps_the_database_cause() {
    let failure: Error = query("gate coverage query")(sqlx::Error::PoolClosed);

    assert_eq!(failure.code(), error_code::INTERNAL_DB_QUERY);
    assert!(!failure.is_rejected());
    assert!(!failure.is_datastore_unavailable());
    assert!(failure.source().is_some());
}

#[test]
fn composed_failures_delegate_classification_and_keep_causes() -> Result<(), &'static str> {
    let database = || {
        afd_db::error::one_of_each_kind()
            .into_iter()
            .find(|(kind, _error)| *kind == "datastore unavailable")
            .map(|(_kind, error)| error)
            .ok_or("database test utility has no outage kind")
    };
    // By name, not by position. `one_of_each_kind` enumerates EVERY kind, and
    // its data-only ones — a missing URL knob, a closed hub — truthfully carry
    // no cause. Taking the first entry made this case's `source()` assertion a
    // hostage to that vector's ordering; "unreachable" is the kind this test
    // actually means, because it is the one that wraps a driver failure.
    let queue = afd_redis::error::one_of_each_kind()
        .into_iter()
        .find(|(kind, _error)| *kind == "unreachable")
        .map(|(_kind, error)| error)
        .ok_or("queue test utility exposes no unreachable error")?;
    let identifier = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("the malformed fixture unexpectedly parsed")?;
    let (entropy, control) = afd_crypto::entropy::Entropy::new_mocked();
    control.fail_next();
    let mut bytes = [0_u8; afd_core::id::ENTROPY_LEN];
    let entropy = entropy
        .fill(&mut bytes)
        .err()
        .ok_or("the controlled entropy source unexpectedly answered")?;

    // The third column is whether the gate is a NEW causal link. Every variant
    // here wraps a failure that happened in another crate, and every one adds
    // the fact that crate cannot carry: WHICH plane was using it. `afd_db`,
    // `afd_redis`, `afd_credential`, `afd_billing`, `afd_crypto` and
    // `afd_core` are each shared by many planes, so "entropy pool exhausted"
    // alone leaves an operator without the one thing they need to act on.
    //
    // `Entropy` and `Identifier` used to be `#[error(transparent)]` and so
    // carried no link, which read as a deliberate distinction and was not one:
    // `code()` and `detail()` already answer for `Queue`, `Entropy` and
    // `Identifier` in a single arm, so three variants identical at the API
    // boundary had two different internal shapes. They now say which plane and
    // keep the inner error as their `source`, like their four siblings.
    //
    // What stays non-uniform is the SECOND column. Only a datastore failure is
    // an availability signal; a refused entropy draw is not, however it is
    // worded.
    let cases = [
        ("database", Error::from(database()?), true, true),
        ("queue", Error::from(queue), false, true),
        (
            "credential",
            Error::from(afd_credential::Error::from(database()?)),
            true,
            true,
        ),
        (
            "billing",
            Error::from(afd_billing::Error::from(database()?)),
            true,
            true,
        ),
        ("entropy", Error::from(entropy), false, true),
        ("identifier", Error::from(identifier), false, true),
    ];

    for (name, failure, unavailable, has_cause) in cases {
        assert_eq!(failure.is_datastore_unavailable(), unavailable, "{name}");
        assert!(!failure.code().as_str().is_empty(), "{name}");
        assert!(!failure.detail().is_empty(), "{name}");
        assert_eq!(
            failure.source().is_some(),
            has_cause,
            "{name} disagrees with its declared causality"
        );
    }
    Ok(())
}
