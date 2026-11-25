//! What the App ingress resolves: an `app_event` row and the repositories it
//! subscribes to.
//!
//! An App delivery is offered to every fleet in a workspace, so a repository
//! list is opt-in rather than opt-out.

use super::{StoredTrigger, binding_of, row};

/// The App subscription row a fleet adds with `trigger_add {kind: "app_event"}`.
fn subscription(repositories: Option<&[&str]>) -> StoredTrigger {
    StoredTrigger {
        repositories: repositories.map(|named| {
            named
                .iter()
                .map(|&repository| repository.to_owned())
                .collect()
        }),
        events: Some(vec!["pull_request".to_owned()]),
        ..row("github")
    }
}

#[test]
fn a_trigger_naming_no_repository_subscribes_to_none() {
    let binding = binding_of(&row("github"));

    assert!(
        !binding.serves_repository("owner/repo"),
        "no repository list is no App subscription, because one App delivery \
         is offered to every fleet in the workspace"
    );
    assert!(
        binding.admits("pull_request"),
        "an absent EVENT list still fires on everything; this pair pins the asymmetry"
    );
}

#[test]
fn a_repository_list_serves_only_what_it_names() {
    let binding = binding_of(&subscription(Some(&["owner/repo"])));

    assert!(binding.serves_repository("owner/repo"));
    assert!(binding.admits("pull_request") && !binding.admits("push"));
    assert!(
        !binding.serves_repository("owner/other"),
        "a fleet must not be woken by a repository its row does not name"
    );
}

/// GitHub treats `Owner/Repo` and `owner/repo` as one repository.
#[test]
fn the_repository_match_ignores_case() {
    let binding = binding_of(&subscription(Some(&["Owner/Repo"])));

    assert!(binding.serves_repository("owner/repo"));
    assert!(binding.serves_repository("OWNER/REPO"));
}

/// A repository the parser refuses is a row no add writes.
#[test]
fn a_malformed_repository_is_refused() {
    let refused = super::resolve(
        &subscription(Some(&["not a repository"])),
        afd_fleet_lifecycle::FleetStatus::Active.as_str(),
    );

    refused.expect_err("a repository that is not owner/name binds nothing");
}
