//! Every refusal carries its registry code and the sentence a client reads.
//!
//! This file checks that the pairing is a function and that no two refusals
//! collapse onto one another by accident. Clients branch on the code, so the
//! code is what is pinned; the sentences are only required to be distinct.
use std::collections::BTreeSet;

use afd_auth::error::{Error, Unavailable};

/// The pairing is a function, and the catalogue is total.
///
/// `ALL` is what every walk below iterates, so a variant missing from it would
/// make this whole file vacuous — the count assertion is what stops that.
#[test]
fn test_every_refusal_has_exactly_one_code_and_one_sentence() {
    assert_eq!(
        Error::ALL.len(),
        7,
        "a new variant must be added to ALL or every walk here silently skips it"
    );

    let mut codes = BTreeSet::new();
    let mut details = BTreeSet::new();
    for err in Error::ALL {
        assert!(
            codes.insert(err.code().as_str()),
            "{err:?} shares a code with an earlier variant"
        );
        assert!(
            details.insert(err.detail()),
            "{err:?} shares its sentence with an earlier variant"
        );
        // `Display` and `detail` must not be two places a sentence is written.
        assert_eq!(err.to_string(), err.detail(), "{err:?}");
        assert!(!err.detail().is_empty(), "{err:?} says nothing");
    }
}

/// Each refusal answers its registry code, spelled exactly as a client matches
/// it.
#[test]
fn test_each_refusal_answers_the_documented_registry_code() {
    for (err, code) in [
        (Error::InvalidOrMissingToken, "UZ-AUTH-002"),
        (Error::TokenExpired, "UZ-AUTH-003"),
        (Error::Unavailable, "UZ-AUTH-004"),
        (Error::TenantKeyRevoked, "UZ-APIKEY-004"),
        (Error::CliCredentialRevoked, "UZ-AUTH-023"),
        (Error::InvalidRunnerToken, "UZ-RUN-001"),
        (Error::RunnerStateBlocked, "UZ-RUN-009"),
    ] {
        assert_eq!(err.code().as_str(), code, "{err:?}");
    }
}

/// Exactly one refusal is not a verdict on the caller.
///
/// The runner client counts consecutive REJECTIONS toward a self-termination
/// ceiling and resets that counter on anything else, so this partition is the
/// difference between a Postgres blip and a fleet walking itself to shutdown.
#[test]
fn test_only_an_outage_is_not_a_rejection() {
    let not_rejections: Vec<_> = Error::ALL
        .into_iter()
        .filter(|err| !err.is_rejection())
        .collect();
    assert_eq!(not_rejections, vec![Error::Unavailable]);
}

/// A dependency's own reason never reaches the decision — every unreachable
/// datastore and every unreachable provider becomes the same refusal.
#[test]
fn test_an_unavailable_dependency_becomes_the_outage_refusal() {
    assert_eq!(Error::from(Unavailable), Error::Unavailable);
    // It renders as what a client would be told, so a log line and a response
    // body cannot disagree about what happened.
    assert_eq!(
        Unavailable.to_string(),
        Error::Unavailable.detail(),
        "the seam error and the refusal must say the same thing"
    );
}

/// A refusal is an `Error`, so it composes with `?` and renders in a chain.
#[test]
fn test_a_refusal_is_an_error() {
    fn as_error(err: Error) -> Box<dyn std::error::Error> {
        Box::new(err)
    }
    for err in Error::ALL {
        assert_eq!(as_error(err).to_string(), err.detail());
    }
}
