//! What a run must say about itself before it is allowed to measure anything.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::collections::BTreeMap;

use super::{DATASTORE_IMAGE_VARIABLE, OWNED_VALUE, OWNED_VARIABLE, Provenance, REVISION_VARIABLE};
use crate::error::Error;

/// The three variables a complete run supplies.
fn complete() -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (REVISION_VARIABLE, "3643941ca".to_owned()),
        (
            DATASTORE_IMAGE_VARIABLE,
            "docker.dragonflydb.io/dragonflydb/dragonfly:v1.40.2".to_owned(),
        ),
        (OWNED_VARIABLE, OWNED_VALUE.to_owned()),
    ])
}

/// Reads `environment`, and nothing else.
fn read(environment: &BTreeMap<&'static str, String>) -> Result<Provenance, Error> {
    Provenance::read(&|key| environment.get(key).cloned())
}

#[test]
fn a_complete_environment_is_read_whole() {
    let provenance = read(&complete()).expect("a complete environment is readable");
    assert_eq!(provenance.revision, "3643941ca");
    assert_eq!(
        provenance.datastore_image,
        "docker.dragonflydb.io/dragonflydb/dragonfly:v1.40.2"
    );
    assert!(provenance.owned);
}

#[test]
fn every_field_refuses_to_be_absent() {
    // One case per variable rather than one for the set: a check that fires
    // only when all three are missing would pass a run that forgot one, which
    // is the run that actually happens.
    for missing in [REVISION_VARIABLE, DATASTORE_IMAGE_VARIABLE, OWNED_VARIABLE] {
        let mut environment = complete();
        environment.remove(missing);
        let refusal = read(&environment).expect_err("an incomplete environment is refused");
        assert!(
            matches!(refusal, Error::VariableUnset { variable } if variable == missing),
            "{missing} unset must be refused by name, got {refusal:?}"
        );
    }
}

#[test]
fn a_blank_field_is_as_absent_as_an_unset_one() {
    // A shell expanding an unset variable hands the process an empty string,
    // not an absent key, so the empty case is the one a real environment
    // produces.
    for blank in ["", "   "] {
        let mut environment = complete();
        environment.insert(REVISION_VARIABLE, blank.to_owned());
        let refusal = read(&environment).expect_err("a blank field is refused");
        assert!(
            matches!(refusal, Error::VariableUnset { variable } if variable == REVISION_VARIABLE),
            "a blank revision must be refused, got {refusal:?}"
        );
    }
}

#[test]
fn only_the_exact_word_claims_ownership() {
    // An exported leftover must not turn a shared endpoint into one this run
    // is entitled to reset and inject faults into.
    for not_owned in ["1", "true", "yes", "OWNED", "shared", "owned-by-me"] {
        let mut environment = complete();
        environment.insert(OWNED_VARIABLE, not_owned.to_owned());
        let provenance = read(&environment).expect("a set variable is readable");
        assert!(
            !provenance.owned,
            "{not_owned:?} must not read as ownership"
        );
    }
}

#[test]
fn a_shared_target_is_a_statement_and_not_a_silence() {
    // Saying "shared" is allowed; saying nothing is not. The difference is
    // whether a person decided, and the grader trusts the field either way.
    let mut environment = complete();
    environment.insert(OWNED_VARIABLE, "shared".to_owned());
    let provenance = read(&environment).expect("a declared shared target is readable");
    assert!(!provenance.owned);
}
