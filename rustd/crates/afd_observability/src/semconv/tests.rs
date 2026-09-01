//! Dimension 1.1 — the vocabulary covers everything this daemon spells.
//!
//! # What these tests deliberately do not do
//!
//! They do not assert that `ATTR_HTTP_ROUTE == "http.route"` for every
//! constant. That is ground truth (`M-TAUTOLOGICAL-TESTS`): it passes by
//! construction, restates the declaration in a second place that can drift,
//! and catches nothing a typo in the test would not also break.
//!
//! What is asserted is the PROPERTY the vocabulary has to hold for the export
//! to be correct — that it covers the contract exactly, in both directions,
//! and that no two keys collide.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use super::{CENSUS_LABEL_KEYS, DELIVERY_SPAN_KEYS};
use crate::metrics::registry::Registry;

/// The namespaces a key this daemon emits may live under.
///
/// `gen_ai.` is the pinned upstream vocabulary; `agentsfleet.` is ours. A key
/// under neither is a bare word that will collide with somebody else's.
const OWNED_NAMESPACES: [&str; 2] = ["gen_ai.", "agentsfleet."];

/// Every label key the compiled-in census actually declares.
fn declared_labels() -> BTreeSet<String> {
    Registry::declared()
        .expect("the compiled-in census reads")
        .families()
        .flat_map(|family| family.labels.iter().map(|label| label.to_string()))
        .collect()
}

/// The census names no label the vocabulary is missing, and vice versa.
///
/// Both directions matter and they fail differently. A column with no constant
/// is a key some producer is about to spell by hand, which is how one emitter
/// ends up writing `pool_result` and another `poolResult` — two dimensions in
/// the backend and no error anywhere. A constant with no column is the reverse
/// defect: a key nothing exports, which reads as coverage and is dead code.
#[test]
fn every_census_label_resolves_to_a_constant() {
    let declared = declared_labels();
    let vocabulary: BTreeSet<String> = CENSUS_LABEL_KEYS
        .iter()
        .map(|key| (*key).to_string())
        .collect();

    let unspelled: Vec<&String> = declared.difference(&vocabulary).collect();
    assert!(
        unspelled.is_empty(),
        "the census declares label columns the vocabulary does not spell: {unspelled:?}"
    );

    let unexported: Vec<&String> = vocabulary.difference(&declared).collect();
    assert!(
        unexported.is_empty(),
        "the vocabulary spells label keys no census family declares: {unexported:?}"
    );
}

/// No key is declared twice under two names.
///
/// A duplicate is not a tidiness problem. Two constants holding one string are
/// two dimensions a reader believes are separate, and the backend merges them
/// silently — the failure arrives as a dashboard that adds up wrong, months
/// later, with nothing pointing here.
#[test]
fn no_key_is_spelled_twice() {
    for (name, keys) in [
        ("the census label keys", CENSUS_LABEL_KEYS),
        ("the delivery span keys", DELIVERY_SPAN_KEYS),
    ] {
        let unique: BTreeSet<&&str> = keys.iter().collect();
        assert_eq!(
            unique.len(),
            keys.len(),
            "{name} carry a repeated spelling: {keys:?}"
        );
    }
}

/// Every delivery-span key lives in a namespace this product owns.
#[test]
fn every_delivery_span_key_is_namespaced() {
    for key in DELIVERY_SPAN_KEYS {
        assert!(
            OWNED_NAMESPACES
                .iter()
                .any(|namespace| key.starts_with(namespace)),
            "`{key}` is under no namespace this daemon owns, so it will collide \
             with whatever else claims the bare word"
        );
    }
}
