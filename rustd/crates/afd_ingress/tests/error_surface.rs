//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a person reads at three in the morning, and they are
//! the easiest to leave untested because the happy path never touches them.
//! `afd_connector`, `afd_cron`, `afd_db` and `afd_redis` each carry the same
//! suite over the same sample seam; this crate had neither.
//!
//! # The distinction this crate must not lose
//!
//! Nothing a SENDER did wrong is an error here. A delivery with no signature, a
//! stale timestamp, a fleet whose workspace configured no secret — none of them
//! reach this type, because nothing failed: the wall did its job and the
//! ingress answered a refusal. What lives here is the other half, THIS side
//! being broken, and keeping them apart is what stops an operator's alert
//! firing every time a scanner probes `/v1/webhooks/{id}` (RULE ECL).
//!
//! # A queue outage and a queue that answered are not the same failure
//!
//! `is_datastore_unavailable` is the question the HTTP edge turns on: an outage
//! is a 503 that a sender should retry, and every other failure here is a 500
//! that retrying will not fix. Both halves of that decision live behind the one
//! `Queue` variant, so the sample carries one of each and the tests below read
//! them apart.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::error_code;
use afd_ingress::error::{detail, one_of_each_kind};

/// The labels the sample gives the failures a retry could plausibly fix.
///
/// Named here rather than derived from the errors themselves: a test that asked
/// the type which kinds are outages and then asserted the answer would agree
/// with any answer. This is the list a person maintains, so a variant that
/// changes sides has to be moved by hand.
const OUTAGES: &[&str] = &["datastore", "queue unreachable"];

#[test]
fn every_kind_renders_leading_with_its_code() {
    let kinds = one_of_each_kind();
    assert!(
        kinds.len() >= 8,
        "a kind was added to ErrorKind without a sample beside it"
    );

    for (label, error) in &kinds {
        let rendered = error.to_string();
        assert!(
            rendered.starts_with(&format!("[{}]", error.code().as_str())),
            "{label} does not lead with its code: {rendered}"
        );
        assert!(
            rendered.len() > error.code().as_str().len() + 3,
            "{label} renders its code and nothing else"
        );
    }
}

#[test]
fn no_error_reports_itself_as_its_own_cause() {
    // `RUST_ERROR_STANDARD` rule 4 as the standard states it — not "every
    // variant has a source", which that document explicitly calls wrong.
    // `RowUnreadable` holds nothing and nothing caused it in the sense a chain
    // walker means.
    for (label, error) in one_of_each_kind() {
        let rendered = error.to_string();
        if let Some(source) = error.source() {
            assert_ne!(
                source.to_string(),
                rendered,
                "{label} reports itself as its own cause"
            );
            assert!(
                !rendered.ends_with(&source.to_string()),
                "{label} repeats its source verbatim: {rendered}"
            );
        }
    }
}

#[test]
fn only_a_datastore_that_could_not_be_reached_reports_an_outage() {
    // The 503-versus-500 decision, and the reason `Queue` is sampled twice. A
    // queue that is GONE is the outage a sender retries against; a queue that
    // answered and refused will answer the same way forever, and telling a
    // sender to retry it is outbound load with no end.
    for (label, error) in one_of_each_kind() {
        let expected = OUTAGES.contains(&label);
        assert_eq!(
            error.is_datastore_unavailable(),
            expected,
            "{label} is on the wrong side of the retry decision"
        );
    }
}

#[test]
fn an_outage_answers_the_unavailable_code_and_everything_else_does_not() {
    for (label, error) in one_of_each_kind() {
        if OUTAGES.contains(&label) {
            assert_eq!(
                error.code(),
                error_code::INTERNAL_DB_UNAVAILABLE,
                "{label} is an outage and must answer the code a retry reads"
            );
            assert_eq!(error.detail(), detail::DATABASE_UNAVAILABLE);
        } else {
            assert_ne!(
                error.code(),
                error_code::INTERNAL_DB_UNAVAILABLE,
                "{label} is not an outage and must not invite a retry"
            );
        }
    }
}

#[test]
fn no_sentence_names_which_internal_failure_it_was() {
    // Three different internal failures share one sentence on purpose. Naming
    // which of them it was would tell whoever provoked it something about this
    // deployment's stored state, and a webhook sender is exactly the caller who
    // must not learn it — the endpoint is public and unauthenticated until the
    // signature passes.
    let opaque: Vec<&'static str> = one_of_each_kind()
        .into_iter()
        .filter(|(label, _error)| ["vault", "queue answered", "config unreadable"].contains(label))
        .map(|(_label, error)| error.detail())
        .collect();

    assert_eq!(
        opaque.len(),
        3,
        "the sample lost one of the opaque failures"
    );
    let distinct: std::collections::BTreeSet<&&str> = opaque.iter().collect();
    assert_eq!(
        distinct.len(),
        1,
        "three internal failures must be indistinguishable to a sender: {opaque:?}"
    );
    assert_eq!(
        opaque.first().copied(),
        Some(detail::OPERATION_FAILED),
        "and the one sentence they share is the generic one"
    );
}

#[test]
fn a_row_this_build_cannot_read_names_the_column_to_an_operator_only() {
    // Both halves matter and they point opposite ways. The operator needs to
    // know WHICH column to go and look at, so the rendering carries it; the
    // sender must not, so the sentence does not.
    for (label, error) in one_of_each_kind() {
        if !label.starts_with("row unreadable") {
            continue;
        }
        let column = label
            .strip_prefix("row unreadable ")
            .expect("the sample labels each column");
        assert!(
            error.to_string().contains(column),
            "{label} does not name its column to the operator: {error}"
        );
        assert!(
            !error.detail().contains(column),
            "{label} leaks its column into the sender's sentence: {}",
            error.detail()
        );
        assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY);
    }
}
