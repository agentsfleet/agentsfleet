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

    // The third column is whether the gate is a NEW causal link, and it is not
    // uniform — demanding a cause from every variant would demand an invented
    // one. `Datastore`, `Queue`, `Credential`, and `Billing` each add a
    // gate-voiced sentence over a failure that happened elsewhere, so the
    // inner error is their `source`. `Entropy` and `Identifier` are
    // `#[error(transparent)]`: the gate adds no sentence, so it adds no link,
    // and `source()` forwards to the inner error's own — which is `None`,
    // because a refused entropy draw and a malformed identifier are data, not
    // consequences of some other failure.
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
        ("entropy", Error::from(entropy), false, false),
        ("identifier", Error::from(identifier), false, false),
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
