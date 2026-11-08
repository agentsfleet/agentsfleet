//! What the App ingress resolves: the trigger its provider selects, and the
//! repositories that trigger subscribes to.
//!
//! The provider is in the App URL, so selection is by source rather than by
//! declaration order, and an App delivery is offered to every fleet in a
//! workspace, so a repository list is opt-in rather than opt-out.

use afd_fleet_lifecycle::FleetStatus;

use super::{Binding, FLEET, READABLE, WORKSPACE, binding_of, document, id};

/// The resolution a named provider gets, for the App-ingress tests.
fn read_source(triggers: &str, source: &str) -> Option<Binding> {
    Binding::read_for_source(
        id(FLEET),
        id(WORKSPACE),
        FleetStatus::Active.as_str(),
        &document(triggers),
        source,
    )
    .expect(READABLE)
}

/// Source selection is independent of declaration order.
#[test]
fn the_source_selects_the_trigger_rather_than_declaration_order() {
    let both = r#"[
        {"type":"webhook","source":"slack","events":["message"]},
        {"type":"webhook","source":"github","repositories":["owner/repo"],"events":["pull_request"]}
    ]"#;

    let named = read_source(both, "github").expect("a github webhook trigger is declared");
    assert_eq!(named.source(), "github");
    assert!(
        named.admits("pull_request") && !named.admits("message"),
        "the allow-list read must be the named provider's own"
    );
}

#[test]
fn a_source_no_trigger_declares_resolves_to_nothing() {
    let github = r#"[{"type":"webhook","source":"github","repositories":["owner/repo"]}]"#;

    assert!(
        read_source(github, "linear").is_none(),
        "a delivery from a provider this fleet never declared reaches no \
         binding, and so wakes nothing"
    );
}

/// The document's source is a lowercase slug, so case can only differ on the
/// URL's side; a provider capitalising its own name there still reaches it.
#[test]
fn the_source_match_ignores_the_urls_case() {
    let github = r#"[{"type":"webhook","source":"github","repositories":["owner/repo"]}]"#;

    assert!(
        read_source(github, "GitHub").is_some(),
        "a URL's capitalisation is not a subscription this daemon may drop"
    );
}

#[test]
fn a_trigger_naming_no_repository_subscribes_to_none() {
    let github = r#"[{"type":"webhook","source":"github"}]"#;

    let binding = binding_of(github);

    assert!(
        !binding.serves_repository("owner/repo"),
        "no repository list is no App subscription — the opposite of an \
         absent event list, because one App delivery is offered to every fleet \
         in the workspace and silence must not opt a fleet into another team's \
         repository"
    );
    assert!(
        binding.admits("pull_request"),
        "an absent EVENT list still fires on everything, which is the \
         asymmetry this pair of assertions pins"
    );
}

#[test]
fn a_repository_list_serves_only_what_it_names() {
    let github = r#"[{"type":"webhook","source":"github","repositories":["owner/repo"]}]"#;

    let binding = binding_of(github);

    assert!(binding.serves_repository("owner/repo"));
    assert!(
        !binding.serves_repository("owner/other"),
        "a fleet must not be woken by a repository nobody bound it to"
    );
}

/// GitHub treats `Owner/Repo` and `owner/repo` as one repository.
///
/// A case-sensitive match here would drop deliveries for a subscription an
/// author could stare straight at without seeing what was wrong.
#[test]
fn the_repository_match_ignores_case() {
    let github = r#"[{"type":"webhook","source":"github","repositories":["Owner/Repo"]}]"#;

    let binding = binding_of(github);

    assert!(binding.serves_repository("owner/repo"));
    assert!(binding.serves_repository("OWNER/REPO"));
}
