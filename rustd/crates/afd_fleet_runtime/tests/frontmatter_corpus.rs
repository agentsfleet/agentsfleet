//! The committed frontmatter corpus, re-run against the Rust parser.
//!
//! `frontmatter_fixtures_test.zig` loads every document under
//! `tests/fixtures/fleetbundle/` and pins the verdict its parser reaches. This
//! file loads the SAME files from the SAME place and pins the same verdicts, so
//! the corpus is one oracle two daemons answer to rather than two corpora that
//! can drift. Nothing here compiles Zig; the parity claim is carried by both
//! suites agreeing about the same bytes on disk.
//!
//! # Mapping
//!
//! | Zig test (`frontmatter_fixtures_test.zig`) | Rust test here |
//! |---|---|
//! | every `skill/` and `trigger/` fixture verdict | [`test_fleet_frontmatter_corpus_parity`] |
//! | the `platform-ops` / `steer-probe` template substitution | [`the_templated_bundles_parse_once_their_placeholders_are_filled`] |
//!
//! The FIELD-VALUE half of the corpus lives in `frontmatter_fields.rs`; this
//! file asserts only which documents open and which are refused.
//!
//! # Why the JSON is compared as values and not as bytes
//!
//! `yaml_frontmatter.zig` writes `", "` between entries and `": "` after a key;
//! serde writes neither. Nothing downstream can see the difference — the bytes
//! are bound as `$6::jsonb` and Postgres normalises whitespace and key order on
//! the way in — so asserting on spacing would pin a property the product does
//! not have. What must agree is the VALUE, and that is what is asserted.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::{Error, parse_skill, parse_trigger};

use crate::support::{MODEL_VALUE, fixture, raw_fixture};

/// What the corpus expects one document to answer, in the ZIG's vocabulary.
///
/// The corpus is `config_markdown.zig`'s oracle, so the verdict a row asserts
/// is the `FleetConfigError` that parser reaches. The Rust error set is FINER
/// — a missing fence, unreadable YAML, a duplicated key and a wrong-typed
/// field are four variants here and one there — so [`zig_class`] folds the
/// Rust answer back into the Zig's vocabulary before comparing. Asserting the
/// Rust variant directly would pin this suite to a spelling the oracle does
/// not have, and would go green on a document the Zig refuses for a different
/// reason entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The document parses.
    Accepts,
    /// `FleetConfigError.MissingRequiredField` — the Zig's catch-all, standing
    /// for an absent key AND for every way a document can fail to open.
    MissingRequiredField,
    /// `FleetConfigError.RuntimeKeysOutsideBlock`.
    RuntimeKeysOutsideBlock,
    /// `FleetConfigError.UnknownRuntimeKey`.
    UnknownRuntimeKey,
}

/// The Zig class a Rust refusal folds onto.
///
/// The four that collapse are the milestone's declared divergence, restated as
/// a function so the collapse is visible rather than implied: the Zig maps a
/// missing fence, a tokeniser failure, a duplicated key and a non-scalar key
/// all onto `MissingRequiredField`, and so does this — for the purpose of
/// grading the corpus, and nowhere else.
///
/// [`None`] for a class no corpus row expects, so the caller can name the
/// document rather than fold it into a verdict it did not earn.
const fn zig_class(failure: &Error) -> Option<Verdict> {
    match failure {
        Error::FrontmatterMissing
        | Error::FrontmatterUnreadable { .. }
        | Error::DuplicateKey { .. }
        | Error::NonScalarKey
        | Error::MissingRequiredField { .. }
        | Error::InvalidFieldType { .. } => Some(Verdict::MissingRequiredField),
        Error::RuntimeKeyOutsideBlock { .. } => Some(Verdict::RuntimeKeysOutsideBlock),
        Error::UnknownRuntimeKey { .. } => Some(Verdict::UnknownRuntimeKey),
        // A class the Zig also spells separately. A corpus row reaching one is
        // a row whose expectation somebody has to write down.
        _other => None,
    }
}

/// Which document kind a fixture is, and therefore which parser opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A `SKILL.md`.
    Skill,
    /// A `TRIGGER.md`.
    Trigger,
}

/// Every purpose-built fixture, with the verdict its bytes must earn.
const CORPUS_CASES: [(&str, Kind, Verdict); 9] = [
    ("skill/minimal.md", Kind::Skill, Verdict::Accepts),
    ("skill/full.md", Kind::Skill, Verdict::Accepts),
    // The fixture's own comment says it tests an absent `name`. It does not:
    // its `description` value carries a second `": "` — "the required name:
    // field." — which is not a plain scalar, so BOTH daemons refuse it while
    // tokenising, before any key is looked for. The verdict is parity and the
    // row belongs here; the fixture is a corpus bug reported separately, and
    // `skill::tests::a_missing_name_names_the_key` covers what it meant to.
    (
        "skill/missing_name.md",
        Kind::Skill,
        Verdict::MissingRequiredField,
    ),
    ("trigger/minimal.md", Kind::Trigger, Verdict::Accepts),
    ("trigger/full.md", Kind::Trigger, Verdict::Accepts),
    (
        "trigger/with_model_and_context.md",
        Kind::Trigger,
        Verdict::Accepts,
    ),
    (
        "trigger/runtime_at_top_level.md",
        Kind::Trigger,
        Verdict::RuntimeKeysOutsideBlock,
    ),
    (
        "trigger/unknown_runtime_key.md",
        Kind::Trigger,
        Verdict::UnknownRuntimeKey,
    ),
    ("steer-probe/SKILL.md", Kind::Skill, Verdict::Accepts),
];

/// The verdict a document actually earns.
fn verdict_of(relative: &str, kind: Kind) -> Verdict {
    let source = fixture(relative);
    let failure = match kind {
        Kind::Skill => match parse_skill(&source) {
            Ok(_parsed) => return Verdict::Accepts,
            Err(failure) => failure,
        },
        Kind::Trigger => match parse_trigger(&source) {
            Ok(_parsed) => return Verdict::Accepts,
            Err(failure) => failure,
        },
    };
    zig_class(&failure)
        .unwrap_or_else(|| panic!("{relative} refused with an unclassified error: {failure}"))
}

/// Every corpus document earns the verdict the Zig suite pins for it.
///
/// One test over the whole table rather than one per file, because a fixture
/// added to the corpus and not to this list is the failure worth catching, and
/// that reads as a missing row rather than a missing function.
#[test]
fn test_fleet_frontmatter_corpus_parity() {
    for (relative, kind, expected) in CORPUS_CASES {
        assert_eq!(
            verdict_of(relative, kind),
            expected,
            "{relative} should answer {expected:?}"
        );
    }
}

/// The templated bundles parse once their placeholders are filled.
///
/// `context_cap_tokens: {{context_cap_tokens}}` is UNQUOTED, so the raw
/// document is a flow-mapping token and a genuine parse error. The Zig suite
/// substitutes before parsing and so does this one — a harness that forgot
/// would report a corpus regression that is really a missing substitution.
#[test]
fn the_templated_bundles_parse_once_their_placeholders_are_filled() {
    for relative in ["platform-ops/TRIGGER.md", "steer-probe/TRIGGER.md"] {
        let parsed = parse_trigger(&fixture(relative))
            .unwrap_or_else(|failure| panic!("{relative} should parse: {failure}"));

        assert_eq!(parsed.config().model(), Some(MODEL_VALUE), "{relative}");
    }

    let skill = parse_skill(&fixture("platform-ops/SKILL.md")).expect("a usable document");
    assert_eq!(skill.name().as_str(), "platform-ops");
}

/// The raw templated document does NOT parse, which is what makes the
/// substitution above load-bearing rather than decorative.
#[test]
fn an_unsubstituted_template_is_refused() {
    let raw = raw_fixture("steer-probe/TRIGGER.md");

    assert!(
        parse_trigger(&raw).is_err(),
        "an unfilled `{{{{context_cap_tokens}}}}` is not a number"
    );
}
