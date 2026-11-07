//! Why a scrub could not be built.
//!
//! The one failure composes the matcher's refusal as its source, so the error
//! takes the `afd_core::error_shell!` hull.

use afd_core::error_code::{self, ErrorCode};

afd_core::error_shell!(
    /// A scrub that could not be built, with the backtrace of where.
    pub struct Error(ErrorKind);
);

/// Every way building a scrub fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The matcher could not be built over the secret values.
    #[error("the secret scrub could not be built")]
    Matcher {
        /// The matcher's refusal.
        #[from]
        source: aho_corasick::BuildError,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Matcher { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

afd_core::error_lifts!(Error, ErrorKind:
    aho_corasick::BuildError => Matcher,
);

/// How long the one sample pattern is: the shortest that overflows the
/// matcher's state identifiers on the build [`one_of_each_kind`] asks for.
///
/// A deterministic finite automaton (DFA) multiplies each state's identifier
/// by a 256-wide transition row, so it passes `i32::MAX` at 2^23 states, and
/// `StartKind::Both` gives every state of the pattern's 2^22 a second copy.
#[cfg(feature = "test-util")]
const OVERFLOWING_PATTERN_LEN: usize = 1 << 22;

/// One error of every kind, for the suites that lift this crate's error.
///
/// The matcher's refusal is provoked, never forged: `aho_corasick` builds its
/// error crate-privately, and [`crate::Scrub::of`] meets it only past
/// gigabytes of secret values. A four-mebibyte pattern under the build that
/// overflows soonest raises the same `BuildError` in about a second.
#[cfg(feature = "test-util")]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    use aho_corasick::{AhoCorasick, AhoCorasickKind, StartKind};

    let pattern: Vec<u8> = (0..=u8::MAX)
        .cycle()
        .take(OVERFLOWING_PATTERN_LEN)
        .collect();
    let refused = AhoCorasick::builder()
        .kind(Some(AhoCorasickKind::DFA))
        .byte_classes(false)
        .start_kind(StartKind::Both)
        .build([pattern])
        .expect_err("a DFA past i32::MAX state identifiers refuses to build");
    vec![("matcher", Error::from(refused))]
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    use std::error::Error as _;

    use afd_core::error_code;

    use super::one_of_each_kind;

    #[test]
    fn the_matcher_refusal_is_an_operation_failure_that_keeps_its_cause() {
        let samples = one_of_each_kind();
        let labels: Vec<&str> = samples.iter().map(|(label, _error)| *label).collect();
        assert_eq!(labels, ["matcher"]);
        for (_label, error) in &samples {
            assert_eq!(error.code(), error_code::INTERNAL_OPERATION_FAILED);
            assert_eq!(
                error.source().map(ToString::to_string),
                Some(
                    "state identifier overflow: failed to create state ID \
                     from 2147484416, which exceeds the max of 2147483646"
                        .to_owned()
                )
            );
        }
    }
}
