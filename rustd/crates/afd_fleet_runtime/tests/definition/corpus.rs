//! The committed native setup corpus, and the verdict each document earns.
//!
//! Every Definition under `tests/fixtures/fleets/` is loaded here and
//! pinned to the verdict the shared parser reaches. The table below is the
//! whole claim; the FIELD-VALUE half lives in `fields.rs`.

#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::config::Reach;
use afd_fleet_runtime::{CredentialName, Definition};

use crate::support::{FIRST_PARTY, MODEL_VALUE, definition, definition_path, fixture, raw_fixture};

/// What one corpus document must answer, named by the sentence a reviewer
/// reads rather than a crate-private kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The document parses.
    Accepts,
    /// Refused, and the refusal names this text.
    Refuses(&'static str),
}

/// Every purpose-built document, with the verdict its bytes must earn.
const CORPUS_CASES: [(&str, Verdict); 6] = [
    ("definition/minimal.json", Verdict::Accepts),
    ("definition/full.json", Verdict::Accepts),
    ("definition/with_model_and_context.json", Verdict::Accepts),
    (
        "definition/missing_name.json",
        Verdict::Refuses("`name` is required"),
    ),
    (
        "definition/runtime_at_top_level.json",
        Verdict::Refuses("belongs under the `x-agentsfleet` block"),
    ),
    (
        "definition/unknown_runtime_key.json",
        Verdict::Refuses("`contxt` is not a known `x-agentsfleet` key"),
    ),
];

/// Every corpus document earns the verdict its row pins for it.
///
/// One test over the whole table rather than one per file, because a fixture
/// added to the corpus and not to this list reads as a missing row.
#[test]
fn test_definition_corpus_verdicts() {
    for (relative, expected) in CORPUS_CASES {
        match (Definition::parse(&fixture(relative)), expected) {
            (Ok(_parsed), Verdict::Accepts) => {}
            (Err(failure), Verdict::Refuses(names)) => assert!(
                failure.to_string().contains(names),
                "{relative} should name `{names}`: {failure}"
            ),
            (outcome, _expected) => {
                panic!("{relative} answered {outcome:?}, expected {expected:?}")
            }
        }
    }
}

/// Every first-party fleet's `config.name` agrees with its directory, and the
/// fleet reaches its services through `http_request`.
#[test]
fn every_first_party_definition_agrees_with_its_directory_name() {
    for slug in FIRST_PARTY {
        let parsed = definition(&definition_path(slug));

        assert_eq!(parsed.runtime().name().as_str(), slug, "{slug} config name");
        assert!(
            parsed
                .runtime()
                .tools()
                .iter()
                .any(|tool| &**tool == "http_request"),
            "{slug} declares http_request, found {:?}",
            parsed.runtime().tools()
        );
        assert!(
            parsed.validate_for_save().is_ok(),
            "{slug} carries the instructions Save requires"
        );
    }
}

/// The templated setups parse once their placeholders are filled.
#[test]
fn the_templated_definitions_parse_once_their_placeholders_are_filled() {
    for slug in ["platform-ops", "steer-probe"] {
        let parsed = definition(&definition_path(slug));

        assert_eq!(parsed.runtime().model(), Some(MODEL_VALUE), "{slug}");
    }
}

/// The raw templated document does NOT parse, which is what makes the
/// substitution above load-bearing rather than decorative.
#[test]
fn an_unsubstituted_template_is_refused() {
    let raw = raw_fixture(&definition_path("steer-probe"));

    assert!(
        Definition::parse(&raw).is_err(),
        "a quoted context-ceiling placeholder is not a number"
    );
}

#[test]
fn responder_definition_holds_no_write_reach() {
    let parsed = definition(&definition_path("ci-responder"));
    let config = parsed.runtime();
    for required in ["http_request", "memory_store", "memory_recall"] {
        assert!(
            config.tools().iter().any(|tool| &**tool == required),
            "responder needs {required}"
        );
    }
    let binding = config.repository_binding().expect("a repository binding");
    assert_eq!(binding.reach(), &Reach::Read);
    assert_eq!(
        binding
            .repositories()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["agentsfleet/linkwarden"]
    );
    let network = config.network().expect("a network policy");
    assert!(network.read_only());
    assert_eq!(network.read_post_paths(), []);
    let hosts: Vec<&str> = network.allow().iter().map(AsRef::as_ref).collect();
    assert_eq!(
        hosts,
        [
            "api.github.com",
            "productionresultssa6.blob.core.windows.net",
            "productionresultssa3.blob.core.windows.net"
        ]
    );
    let credentials: Vec<&str> = config
        .credentials()
        .iter()
        .map(CredentialName::as_str)
        .collect();
    assert_eq!(credentials, ["github"]);
}

#[test]
fn log_reader_definition_is_an_isolated_read_only_probe() {
    let parsed = definition(&definition_path("github-log-reader"));
    let config = parsed.runtime();
    assert_eq!(
        config
            .tools()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["http_request"]
    );
    assert_eq!(
        config
            .credentials()
            .iter()
            .map(CredentialName::as_str)
            .collect::<Vec<_>>(),
        ["github"]
    );
    assert_eq!(
        config.repository_binding().expect("binding").reach(),
        &Reach::Read
    );
    let network = config.network().expect("policy");
    assert!(network.read_only());
    assert_eq!(network.read_post_paths(), []);
    assert_eq!(
        network
            .allow()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        [
            "api.github.com",
            "productionresultssa6.blob.core.windows.net"
        ]
    );
    parsed
        .validate_for_save()
        .expect("isolated reader is ready to save");
}

#[test]
fn repairer_definition_is_write_bound_to_one_base() {
    let parsed = definition(&definition_path("ci-repairer"));
    let config = parsed.runtime();
    assert_eq!(
        config
            .tools()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["http_request"]
    );
    let binding = config.repository_binding().expect("a binding");
    assert_eq!(binding.reach(), &Reach::Write { base: "dev".into() });
    assert_eq!(
        binding
            .repositories()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["agentsfleet/linkwarden"]
    );
    let network = config.network().expect("a network policy");
    assert_eq!(
        network
            .allow()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        [
            "api.github.com",
            "productionresultssa6.blob.core.windows.net",
            "productionresultssa3.blob.core.windows.net"
        ]
    );
    assert!(network.read_only());
    assert_eq!(network.read_post_paths(), []);
}
