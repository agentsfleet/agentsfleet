//! What a stored trigger row resolves to, proved with no datastore in the test.
//!
//! [`Binding::read`] takes a status and a row and answers, so every rule this
//! file covers — which vault key a row names, which recipe it selects, whether
//! an event is admitted, whether the fleet will take work — is reachable
//! without a Postgres, a bus or a vault.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;

use super::{Binding, StoredTrigger};

mod app;

/// What a resolution that reached the reader at all has proved.
const READABLE: &str = "the row parses and the status is one this build knows";

/// A canonical identifier, so the tests carry no invalid one by accident.
const FLEET: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8e9f";
/// See [`FLEET`].
const WORKSPACE: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8ea0";

/// A row for `source` with nothing else set.
fn row(source: &str) -> StoredTrigger {
    StoredTrigger {
        source: source.to_owned(),
        ..StoredTrigger::default()
    }
}

/// A fixture identifier, parsed once so no test spells the check twice.
fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the fixture identifiers are canonical")
}

/// The resolution INCLUDING its failure arm, for the tests that expect one.
fn resolve(row: &StoredTrigger, status: &str) -> crate::error::Result<Option<Binding>> {
    Binding::read(id(FLEET), id(WORKSPACE), status, row)
}

/// The binding an active fleet's row resolves to, for the tests that expect one.
fn binding_of(row: &StoredTrigger) -> Binding {
    resolve(row, FleetStatus::Active.as_str())
        .expect(READABLE)
        .expect("this row names a recipe this daemon verifies")
}

#[test]
fn the_vault_key_is_the_source_when_the_row_names_none() {
    assert_eq!(
        binding_of(&row("linear")).credential_name(),
        "linear",
        "the provider's own name IS the key when the row names none"
    );
}

/// A row names its own key so two fleets on one provider hold different
/// secrets; reading the source instead would make both share one.
#[test]
fn a_rows_secret_name_is_the_vault_key() {
    let named = StoredTrigger {
        secret_name: Some("sentry_hook".to_owned()),
        verifier_scheme: Some("body-hex-sha256".to_owned()),
        verifier_header: Some("Sentry-Hook-Signature".to_owned()),
        ..row("sentry")
    };

    let binding = binding_of(&named);

    assert_eq!(binding.source(), "sentry", "the provider is unchanged");
    assert_eq!(binding.credential_name(), "sentry_hook");
    assert_eq!(binding.family(), af_webhook::Family::BodyHexSha256);
    assert_eq!(
        binding.signature_header(),
        Some("sentry-hook-signature"),
        "a declared header is read lowercase, as the wall compares it"
    );
}

/// A custom source is measured with its stored verifier.
#[test]
fn a_custom_source_resolves_to_its_declared_recipe() {
    let jira = StoredTrigger {
        secret_name: Some("jira_svix".to_owned()),
        verifier_scheme: Some("svix-v1".to_owned()),
        ..row("jira")
    };

    assert_eq!(binding_of(&jira).family(), af_webhook::Family::SvixV1);
}

#[test]
fn a_registry_source_resolves_to_the_walls_own_scheme() {
    let github = binding_of(&row("github"));

    assert_eq!(github.family(), af_webhook::Family::BodyHexSha256);
    assert_eq!(github.signature_header(), Some("x-hub-signature-256"));
}

/// A row nothing in this daemon writes is an incident, never a binding.
#[test]
fn a_row_with_no_readable_recipe_is_refused() {
    let refused = resolve(&row("acme"), FleetStatus::Active.as_str())
        .expect_err("a custom source with no recipe is a row no add writes");
    let cause = std::error::Error::source(&refused).expect("the parser's refusal is retained");
    assert!(cause.to_string().contains("acme"), "{cause}");
}

#[test]
fn an_absent_event_list_admits_every_event() {
    let binding = binding_of(&row("github"));

    assert!(binding.admits("pull_request"));
    assert!(binding.admits("workflow_run"));
}

#[test]
fn a_rows_event_list_admits_only_what_it_names() {
    let filtered = StoredTrigger {
        events: Some(vec!["workflow_run".to_owned()]),
        ..row("github")
    };

    let binding = binding_of(&filtered);

    assert!(binding.admits("workflow_run"));
    assert!(
        !binding.admits("pull_request"),
        "an event outside the list must not wake the fleet"
    );
}

/// An empty list never reaches [`Binding`], so `admits` needs no arm for one.
#[test]
fn an_empty_event_list_is_refused_rather_than_interpreted() {
    let empty = StoredTrigger {
        events: Some(Vec::new()),
        ..row("github")
    };

    assert!(
        resolve(&empty, FleetStatus::Active.as_str()).is_err(),
        "an empty allow-list is not a row this daemon stores"
    );
}

/// Only `active` takes work, and `archived` is not a refusal here: a
/// delivery to it is answered 200 and dropped by the handler.
#[test]
fn only_an_active_fleet_will_take_the_delivery() {
    for (status, expected) in [(FleetStatus::Active, true), (FleetStatus::Archived, false)] {
        let binding = resolve(&row("github"), status.as_str())
            .expect(READABLE)
            .expect("the row names a registry source");

        assert_eq!(binding.is_runnable(), expected, "{}", status.as_str());
    }
}

/// A status this build cannot name is refused, never defaulted.
#[test]
fn a_status_this_build_cannot_name_is_refused() {
    for status in ["installing", "quiesced"] {
        resolve(&row("github"), status).expect_err(status);
    }
}

/// The workspace comes from the ROW, never from the request: a signed
/// delivery carries no principal that could name one.
#[test]
fn the_workspace_is_the_rows_own() {
    let binding = binding_of(&row("github"));

    assert_eq!(binding.workspace().as_str(), WORKSPACE);
    assert_eq!(binding.fleet().as_str(), FLEET);
}
