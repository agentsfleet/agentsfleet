//! Dimension 3.4 — the error type's chain shape.
//!
//! The invariant under test is the one `docs/RUST_ERROR_STANDARD.md` rule 4
//! states, and deliberately NOT the one it warns against: a variant holding
//! only data has no cause, and demanding a `source()` from every variant would
//! force authors to invent one. What is asserted is that where there IS a
//! source, it is not a repeat of our own sentence — a chain walker that prints
//! the same line twice before reaching anything new is the defect the rule
//! exists to prevent.

use core::error::Error as _;

use super::Error;
use crate::metrics::registry::{CENSUS, Registry};

/// The family the data-only fixtures are built around. One name, so a reader
/// can see at a glance that the variants differ in what they report and not in
/// which family they report it for.
const FAMILY: &str = "agentsfleet.api.request.count";

/// A census whose single row is one column short of the header it is read
/// under — the reader's own failure, so the composed variant carries a real
/// chain rather than one a test constructed to satisfy itself.
fn malformed_census() -> String {
    let header: Vec<&str> = CENSUS
        .lines()
        .take_while(|line| line.starts_with('#') || line.starts_with("name\t"))
        .collect();
    header.join("\n") + "\na.family\tcounter\n"
}

fn census_failure() -> Error {
    let Err(error) = Registry::read(&malformed_census()) else {
        unreachable!("a row short of the header's columns must not read");
    };
    error
}

/// One instance of every variant, so a variant added later without a decision
/// about its cause fails this module rather than shipping unexamined.
fn every_variant() -> Vec<Error> {
    vec![
        census_failure(),
        Error::Duplicate {
            family: FAMILY.into(),
            first: 17,
            second: 42,
        },
        Error::BoundsMismatch {
            family: FAMILY.into(),
            kind: "counter",
            bounds: 13,
        },
        Error::StreamRejected {
            family: "agentsfleet.api.request.duration".into(),
            reason: "Cardinality limit must be greater than 0".into(),
        },
        Error::UnknownFamily {
            family: "agentsfleet.nothing.declares.this".into(),
        },
    ]
}

/// Rule 4: a source is what caused us, never a restatement of us.
#[test]
fn test_observability_error_chain_shape() {
    for error in every_variant() {
        let ours = error.to_string();
        assert!(
            !ours.is_empty(),
            "a variant rendered an empty sentence: {error:?}"
        );

        let Some(source) = error.source() else {
            continue;
        };
        assert_ne!(
            ours,
            source.to_string(),
            "a source repeats its own error's sentence, so a chain walker prints \
             it twice before reaching anything new: {error:?}"
        );
    }
}

/// A variant holding only data never invents a cause.
///
/// Deliberately one-directional. The converse — "the composed variant always
/// has one" — is NOT an invariant and asserting it would be the exact mistake
/// the standard names: `#[error(transparent)]` forwards `source()` to the
/// reader error's own cause, and a reader failure like `UnequalLengths` is a
/// leaf that was caused by nothing. Demanding a cause there would force one to
/// be invented.
#[test]
fn test_data_only_variants_invent_no_cause() {
    for error in every_variant() {
        if matches!(error, Error::Census(_)) {
            continue;
        }
        assert!(
            error.source().is_none(),
            "a variant holding only data reported a cause: {error:?}"
        );
    }
}

/// The composed variant's sentence carries the position, because the chain
/// cannot.
///
/// `csv::Error` implements `source()` as the default `None` — it terminates its
/// own chain and puts the record, line and byte in its `Display` instead. That
/// is precisely why this variant is `#[error(transparent)]`: wrapping it in a
/// sentence of ours would bury the only place that information exists.
#[test]
fn test_the_composed_variants_sentence_carries_the_position() {
    let error = census_failure();
    let rendered = error.to_string();
    assert!(
        rendered.contains("line"),
        "the reader's sentence is the only place the position survives: {rendered}"
    );
    assert!(
        error.source().is_none(),
        "the reader is documented to terminate its own chain; if that changed, \
         this crate's transparent forward should be revisited"
    );
}

/// `?` lifts a reader failure with no call-site conversion, which is the whole
/// point of composing by `#[from]` rather than relabelling (`M-FROM-ERROR`).
#[test]
fn test_reader_failures_lift_through_the_question_mark() {
    fn lifting(census: &str) -> super::Result<usize> {
        let registry = Registry::read(census)?;
        Ok(registry.len())
    }
    assert!(matches!(
        lifting(&malformed_census()),
        Err(Error::Census(_))
    ));
}

/// The composed variant forwards the reader's own sentence rather than wrapping
/// it in one of ours. That sentence already names the record and the line, so a
/// restatement would be strictly worse.
#[test]
fn test_the_composed_variant_forwards_its_readers_sentence() {
    let error = census_failure();
    let Error::Census(ref inner) = error else {
        unreachable!("a malformed census fails through the reader");
    };
    assert_eq!(error.to_string(), inner.to_string());
}

/// The alias defaults to this crate's own error, which is what lets a signature
/// be read without checking WHICH error it answers with (rule 1).
#[test]
fn test_result_alias_defaults_to_this_crates_error() {
    fn fallible() -> super::Result<u8> {
        Err(Error::UnknownFamily {
            family: "unasked".into(),
        })
    }
    assert!(matches!(fallible(), Err(Error::UnknownFamily { .. })));
}
