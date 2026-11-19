//! The shared error-surface grade, graded: a sound sample set passes, and each
//! broken rule fails naming the sample that broke it.
#![expect(
    clippy::panic,
    reason = "test target: a grade that passed when it should not fails by name"
)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use af_core::test_util::error_surface::{Expect, grade, grade_sentences};

use self::sample::{Error, Kind, cause};

mod sample {
    use af_core::error_code::{self, ErrorCode};

    /// A crate's failure vocabulary in miniature: one root kind, one wrapping a
    /// cause, one whose sentence repeats its cause, and one answering a code the
    /// registry never declared.
    #[derive(Debug, thiserror::Error)]
    pub(crate) enum Kind {
        #[error("the thing was refused")]
        Root,
        #[error("the store would not answer")]
        Wrapped {
            #[source]
            source: std::io::Error,
        },
        #[error("the store would not answer: {source}")]
        Echoing {
            #[source]
            source: std::io::Error,
        },
        #[error("an unregistered failure")]
        Unregistered,
    }

    af_core::error_shell!(
        /// The sample shell.
        pub struct Error(Kind);
    );

    /// A code nothing in the registry declares.
    const UNDECLARED: ErrorCode = ErrorCode::declare("UZ-TEST-999");

    impl Error {
        pub(crate) const fn code(&self) -> ErrorCode {
            match self.kind() {
                Kind::Root => error_code::INTERNAL_OPERATION_FAILED,
                Kind::Wrapped { .. } | Kind::Echoing { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
                Kind::Unregistered => UNDECLARED,
            }
        }
    }

    pub(crate) fn cause() -> std::io::Error {
        std::io::Error::other("connection reset")
    }
}

fn sound() -> Vec<(&'static str, Error)> {
    vec![
        ("root", Error::from(Kind::Root)),
        ("wrapped", Error::from(Kind::Wrapped { source: cause() })),
    ]
}

#[test]
fn logged_reasons_keep_multiline_causes_on_one_line() {
    use af_core::error_code::Coded as _;
    use std::error::Error as _;

    let failure = Error::from(Kind::Wrapped {
        source: std::io::Error::other("invalid pool\r\nmax_connections: too small\n"),
    });
    assert_eq!(
        failure.logged().reason,
        "the store would not answer: invalid pool max_connections: too small"
    );
    assert_eq!(
        failure.source().map(ToString::to_string).as_deref(),
        Some("invalid pool\r\nmax_connections: too small\n")
    );
}

const SOUND: Expect = Expect {
    with_source: 1,
    distinct_codes: 2,
};

/// Whether `check` panics, with its message.
fn refusal(check: impl FnOnce()) -> Option<String> {
    catch_unwind(AssertUnwindSafe(check)).err().map(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
            })
            .unwrap_or_default()
    })
}

#[test]
fn a_sound_sample_set_passes() {
    grade(&sound(), SOUND);
    // A cause that renders nothing is not a repeated cause.
    let mut silent = sound();
    silent.push((
        "silent cause",
        Error::from(Kind::Wrapped {
            source: std::io::Error::other(""),
        }),
    ));
    grade(
        &silent,
        Expect {
            with_source: 2,
            ..SOUND
        },
    );
    grade_sentences([("root", "It failed"), ("wrapped", "Try again")], 2);
}

#[test]
fn each_broken_rule_fails_naming_the_sample() {
    for (case, check, names) in BROKEN_CASES {
        let message = refusal(check).unwrap_or_else(|| panic!("{case}: the grade passed"));
        assert!(message.contains(names), "{case}: {message}");
    }
}

type BrokenCase = (&'static str, fn(), &'static str);

const BROKEN_CASES: [BrokenCase; 8] = [
    (
        "a moved source count",
        || {
            grade(
                &sound(),
                Expect {
                    with_source: 2,
                    ..SOUND
                },
            );
        },
        "keep a source",
    ),
    (
        "a moved code count",
        || {
            grade(
                &sound(),
                Expect {
                    distinct_codes: 1,
                    ..SOUND
                },
            );
        },
        "collapsed",
    ),
    (
        "a kind repeating its cause",
        || {
            let mut samples = sound();
            samples.push(("echoing", Error::from(Kind::Echoing { source: cause() })));
            grade(
                &samples,
                Expect {
                    with_source: 2,
                    ..SOUND
                },
            );
        },
        "echoing repeats its own cause",
    ),
    (
        "an undeclared code",
        || {
            let mut samples = sound();
            samples.push(("unregistered", Error::from(Kind::Unregistered)));
            grade(
                &samples,
                Expect {
                    distinct_codes: 3,
                    ..SOUND
                },
            );
        },
        "unregistered reports UZ-TEST-999",
    ),
    ("no samples", || grade::<Error>(&[], SOUND), "no samples"),
    (
        "a blank sentence",
        || grade_sentences([("root", " ")], 1),
        "root hands the caller an empty sentence",
    ),
    (
        "a coded sentence",
        || grade_sentences([("root", "[UZ-AUTH-001] no")], 1),
        "root leaks a registry code",
    ),
    (
        "a moved vocabulary",
        || grade_sentences([("a", "One"), ("b", "One")], 2),
        "sentences handed to callers",
    ),
];
