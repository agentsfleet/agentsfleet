//! One grading of a crate's error surface, fed that crate's samples.
//!
//! Every crate built on [`crate::error_shell!`] exports `one_of_each_kind()`,
//! one sample per `ErrorKind` variant. What must hold for those samples is the
//! same everywhere, so it is written once here and each crate's
//! `tests/error_surface.rs` calls it with its own measured counts, keeping only
//! the checks that are genuinely that crate's: its predicates and its
//! caller-facing vocabulary.

use std::collections::BTreeSet;

use crate::error_code::{Coded, REGISTRY};

/// What a crate's sample set measures, pinned so a change disagrees with it.
#[derive(Debug, Clone, Copy)]
pub struct Expect {
    /// How many samples carry a `source()`: the chain an operator follows. A
    /// lift replaced by a `to_string()` compiles and reads the same at the
    /// call site, and lowers this.
    pub with_source: usize,
    /// How many distinct registry codes the kinds report between them: what a
    /// caller can discriminate on.
    pub distinct_codes: usize,
}

/// Grades `samples` against everything every error surface owes.
///
/// - every code is one the registry declares;
/// - `Display` leads with `[CODE]` and says something after it;
/// - no error reports itself as its own cause, and no kind repeats its
///   source's text (`RUST_ERROR_STANDARD` rule 4), which reads as a truncated
///   chain to whoever follows it;
/// - the source and code counts are the ones `expect` pins.
///
/// # Panics
/// On the first sample that breaks a rule, naming it, or on a count that
/// moved: a test's failure is this function's whole output.
pub fn grade<E: Coded>(samples: &[(&str, E)], expect: Expect) {
    assert!(!samples.is_empty(), "the crate exports no samples");
    let mut codes = BTreeSet::new();
    let mut wrapped = Vec::new();
    for (label, error) in samples {
        let code = error.code();
        assert!(
            REGISTRY.contains(&code),
            "{label} reports {} which is not in the registry",
            code.as_str()
        );
        codes.insert(code);

        let rendered = error.to_string();
        let prefix = format!("[{}] ", code.as_str());
        assert!(
            rendered.starts_with(&prefix) && rendered.len() > prefix.len(),
            "{label} does not render as `[CODE] message`: {rendered}"
        );

        if let Some(source) = error.source() {
            wrapped.push(*label);
            // A cause that renders nothing (an empty validation report) has no
            // text to repeat, and every string ends with the empty one.
            let cause = source.to_string();
            assert!(
                cause.is_empty() || !rendered.ends_with(&cause),
                "{label} repeats its own cause: {rendered}"
            );
        }
    }
    assert_eq!(
        wrapped.len(),
        expect.with_source,
        "{wrapped:?} keep a source; a lift was replaced by a stringify, or a sample \
         stopped carrying the cause it was built to carry"
    );
    assert_eq!(
        codes.len(),
        expect.distinct_codes,
        "the kinds collapsed onto a different number of codes than {}, which changes \
         what a caller can discriminate on",
        expect.distinct_codes
    );
}

/// Grades the sentences a crate hands its callers, one per sample.
///
/// Each is present and carries no registry code: `detail()` is what a person is
/// handed, and a code leaking into it is internal phrasing in an API answer.
/// The set is pinned by size, because a new sentence means a caller reading
/// for the old one stops matching.
///
/// # Panics
/// On a blank or coded sentence, or when the vocabulary's size moved.
pub fn grade_sentences<'s>(
    details: impl IntoIterator<Item = (&'s str, &'s str)>,
    vocabulary: usize,
) {
    let mut sentences = BTreeSet::new();
    for (label, detail) in details {
        assert!(
            !detail.trim().is_empty(),
            "{label} hands the caller an empty sentence"
        );
        assert!(
            !detail.contains("[UZ-"),
            "{label} leaks a registry code into the caller's sentence: {detail}"
        );
        sentences.insert(detail);
    }
    assert_eq!(
        sentences.len(),
        vocabulary,
        "the sentences handed to callers are now {sentences:?}"
    );
}
