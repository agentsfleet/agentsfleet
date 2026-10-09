//! The committed frontmatter corpus, and the verdict each document earns.
//!
//! Every document under `tests/fixtures/fleetbundle/` is loaded here and pinned
//! to the verdict this parser reaches. The table below is the whole claim.
//!
//! # Mapping
//!
//! | What is pinned | Test |
//! |---|---|
//! | every `skill/` and `trigger/` fixture verdict | [`test_fleet_frontmatter_corpus_parity`] |
//! | the `platform-ops` / `steer-probe` template substitution | [`the_templated_bundles_parse_once_their_placeholders_are_filled`] |
//! | no first-party bundle promises the retired repository-write card | [`test_fixture_corpus_names_no_retired_gate`] |
//!
//! The FIELD-VALUE half of the corpus lives in `frontmatter_fields.rs`; this
//! file asserts only which documents open and which are refused.
//!
//! # Why the JSON is compared as values and not as bytes
//!
//! serde writes no space after `,` or `:`. Nothing downstream can see spacing —
//! the bytes are bound as `$6::jsonb` and Postgres normalises whitespace and key
//! order on the way in — so asserting on it would pin a property the product
//! does not have. What must agree is the VALUE, and that is what is asserted.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::{Class, Error, config::Access, parse_skill, parse_trigger};

use crate::support::{FIRST_PARTY, MODEL_VALUE, fixture, raw_fixture};

/// The card the daemon retired: the standing integration grant authorises a
/// repository write (`src/config/raw/predicate.rs`), so a bundle promising a
/// per-event card tells its model about a gate that never opens.
const RETIRED_GATE: &str = "approval card";

/// The two documents every first-party bundle ships.
const BUNDLE_DOCUMENTS: [&str; 2] = ["SKILL.md", "TRIGGER.md"];

/// What the corpus expects one document to answer, at the grain the table
/// grades.
///
/// The error set is FINER than this — a missing fence, unreadable YAML, a
/// duplicated key and a wrong-typed field are four variants — so
/// [`corpus_class`] folds each refusal onto one of these before comparing.
/// Asserting the variant directly would pin this table to wording that changes
/// whenever a message is sharpened, when what the corpus guards is which
/// documents open and which are refused for which coarse reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The document parses.
    Accepts,
    /// The document could not be opened or read: an absent key, a missing or
    /// unclosed fence, unreadable YAML, a duplicated key, a wrong-typed value.
    MissingRequiredField,
    /// A runtime key sits at the top level instead of under `x-agentsfleet`.
    RuntimeKeysOutsideBlock,
    /// A key under `x-agentsfleet` is not one the daemon knows.
    UnknownRuntimeKey,
}

/// The verdict a refusal folds onto.
///
/// A function so the collapse is visible rather than implied: a missing fence,
/// a tokeniser failure, a duplicated key and a non-scalar key all fold onto
/// `MissingRequiredField` — for the purpose of grading the corpus, and nowhere
/// else. [`Class`] owns which kind lands in which group.
///
/// [`None`] for a class no corpus row expects, so the caller can name the
/// document rather than fold it into a verdict it did not earn.
fn corpus_class(failure: &Error) -> Option<Verdict> {
    match failure.class() {
        Class::Document => Some(Verdict::MissingRequiredField),
        Class::RuntimeKeyOutsideBlock => Some(Verdict::RuntimeKeysOutsideBlock),
        Class::UnknownRuntimeKey => Some(Verdict::UnknownRuntimeKey),
        // Classes with no verdict of their own here. A corpus row reaching one
        // is a row whose expectation somebody has to write down.
        Class::InvalidCredentialRef | Class::Semantic => None,
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
const CORPUS_CASES: [(&str, Kind, Verdict); 19] = [
    ("skill/minimal.md", Kind::Skill, Verdict::Accepts),
    ("skill/full.md", Kind::Skill, Verdict::Accepts),
    // The fixture's own comment says it tests an absent `name`. It does not:
    // its `description` value carries a second `": "` — "the required name:
    // field." — which is not a plain scalar, so the parser refuses it while
    // tokenising, before any key is looked for. The verdict stands and the
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
    // The three incident bundles, which shipped in a repository-root `library/`
    // for three milestones with no parser reading them at all. They are here so
    // the documents the platform would install are graded by the same table as
    // everything else.
    ("incident-responder/SKILL.md", Kind::Skill, Verdict::Accepts),
    (
        "incident-responder/TRIGGER.md",
        Kind::Trigger,
        Verdict::Accepts,
    ),
    ("incident-repairer/SKILL.md", Kind::Skill, Verdict::Accepts),
    (
        "incident-repairer/TRIGGER.md",
        Kind::Trigger,
        Verdict::Accepts,
    ),
    ("incident-verifier/SKILL.md", Kind::Skill, Verdict::Accepts),
    (
        "incident-verifier/TRIGGER.md",
        Kind::Trigger,
        Verdict::Accepts,
    ),
    ("ci-responder/SKILL.md", Kind::Skill, Verdict::Accepts),
    ("ci-responder/TRIGGER.md", Kind::Trigger, Verdict::Accepts),
    ("ci-repairer/SKILL.md", Kind::Skill, Verdict::Accepts),
    ("ci-repairer/TRIGGER.md", Kind::Trigger, Verdict::Accepts),
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
    corpus_class(&failure)
        .unwrap_or_else(|| panic!("{relative} refused with an unclassified error: {failure}"))
}

/// Every corpus document earns the verdict its row pins for it.
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
/// document is a flow-mapping token and a genuine parse error. This suite
/// substitutes before parsing (`support::fixture`) — a harness that forgot
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

#[test]
fn drill_bundles_parse_and_join_the_corpus() {
    assert_eq!(FIRST_PARTY.len(), 9);
    assert_eq!(CORPUS_CASES.len(), 19);
    for (relative, kind) in [
        ("ci-responder/SKILL.md", Kind::Skill),
        ("ci-responder/TRIGGER.md", Kind::Trigger),
        ("ci-repairer/SKILL.md", Kind::Skill),
        ("ci-repairer/TRIGGER.md", Kind::Trigger),
    ] {
        assert!(
            CORPUS_CASES.contains(&(relative, kind, Verdict::Accepts)),
            "{relative} must be graded by the corpus"
        );
    }
    for slug in ["ci-responder", "ci-repairer"] {
        assert!(
            FIRST_PARTY.contains(&slug),
            "{slug} joins the first-party roster"
        );
        let skill = parse_skill(&fixture(&format!("{slug}/SKILL.md")))
            .unwrap_or_else(|failure| panic!("{slug} skill should parse: {failure}"));
        let trigger = parse_trigger(&fixture(&format!("{slug}/TRIGGER.md")))
            .unwrap_or_else(|failure| panic!("{slug} trigger should parse: {failure}"));
        assert_eq!(skill.name(), trigger.config().name(), "{slug} names agree");
    }
}

#[test]
fn responder_bundle_holds_no_write_reach() {
    let parsed =
        parse_trigger(&fixture("ci-responder/TRIGGER.md")).expect("responder trigger should parse");
    let config = parsed.config();
    assert_eq!(config.tools().len(), 3);
    for required in ["http_request", "memory_store", "memory_recall"] {
        assert!(
            config.tools().iter().any(|tool| &**tool == required),
            "responder needs {required}"
        );
    }
    let binding = config.repository_binding().expect("a repository binding");
    assert_eq!(binding.access(), Access::Read);
    assert_eq!(binding.repositories().len(), 1);
    assert_eq!(
        binding.repositories().first().map(AsRef::as_ref),
        Some("agentsfleet/linkwarden")
    );
    assert_eq!(binding.base_branch(), None);
    let network = config.network().expect("a network policy");
    assert!(network.read_only());
    assert_eq!(network.read_post_paths(), []);
    assert_eq!(network.allow().len(), 2);
    assert!(
        network
            .allow()
            .iter()
            .any(|host| &**host == "api.github.com")
    );
    assert!(
        network
            .allow()
            .iter()
            .any(|host| &**host == "grafana.example.net")
    );
    assert!(!network.allow().iter().any(|host| host.contains("slack")));
    assert_eq!(config.credentials().len(), 2);
    assert!(
        config
            .credentials()
            .iter()
            .any(|name| name.as_str() == "github")
    );
    assert!(
        config
            .credentials()
            .iter()
            .any(|name| name.as_str() == "grafana")
    );
    assert!(
        !config
            .credentials()
            .iter()
            .any(|name| name.as_str() == "slack")
    );
}

#[test]
fn repairer_bundle_is_write_bound_to_one_base() {
    let parsed =
        parse_trigger(&fixture("ci-repairer/TRIGGER.md")).expect("repairer trigger should parse");
    assert_eq!(parsed.config().tools().len(), 1);
    assert_eq!(
        parsed.config().tools().first().map(AsRef::as_ref),
        Some("http_request")
    );
    let binding = parsed.config().repository_binding().expect("a binding");
    assert_eq!(binding.access(), Access::Write);
    assert_eq!(binding.repositories().len(), 1);
    assert_eq!(
        binding.repositories().first().map(AsRef::as_ref),
        Some("agentsfleet/linkwarden")
    );
    assert_eq!(binding.base_branch(), Some("dev"));
    let network = parsed.config().network().expect("a network policy");
    assert_eq!(network.allow().len(), 1);
    assert_eq!(
        network.allow().first().map(AsRef::as_ref),
        Some("api.github.com")
    );
}

#[test]
fn test_fixture_corpus_names_no_retired_gate() {
    let promising: Vec<String> = FIRST_PARTY
        .iter()
        .flat_map(|slug| BUNDLE_DOCUMENTS.map(|document| format!("{slug}/{document}")))
        .filter(|relative| raw_fixture(relative).to_lowercase().contains(RETIRED_GATE))
        .collect();
    assert!(
        promising.is_empty(),
        "these bundles still promise the retired {RETIRED_GATE:?}: {promising:?}"
    );
}
