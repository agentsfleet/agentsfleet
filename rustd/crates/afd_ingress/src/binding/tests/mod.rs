//! What a stored row resolves to, proved with no datastore in the test.
//!
//! [`Binding::read`] takes three strings and answers, so every rule this file
//! covers — which trigger wins, which vault key it names, whether an event is
//! admitted, whether the fleet will take work — is reachable without a
//! Postgres, a Dragonfly or a vault. That is the property worth keeping: the rules
//! a delivery is measured against must not need an environment to test, or
//! they go untested and drift.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;

use super::Binding;

mod app;

/// What a resolution that reached the reader at all has proved.
const READABLE: &str = "the document parses and the status is one this build knows";

/// A canonical identifier, so the tests carry no invalid one by accident.
const FLEET: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8e9f";
/// See [`FLEET`].
const WORKSPACE: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8ea0";

/// A stored document with the `triggers` block a test wants and nothing else
/// varying.
///
/// The surrounding keys are the ones [`afd_fleet_runtime::FleetConfig::stored`]
/// requires — `name`, `tools`, `budget`. Built here rather than loaded from a
/// fixture file so a reader sees the whole input beside the assertion.
fn document(triggers: &str) -> String {
    format!(
        r#"{{
          "name": "ingress-fixture",
          "x-agentsfleet": {{
            "triggers": {triggers},
            "tools": ["bash"],
            "budget": {{ "daily_nanos": "{daily}" }}
          }}
        }}"#,
        daily = afd_core::money::NANOS_PER_USD
    )
}

/// The binding a document resolves to, for the tests that expect one.
fn binding_of(triggers: &str) -> Binding {
    read(triggers, FleetStatus::Active.as_str()).expect("this document declares a webhook trigger")
}

/// A fixture identifier, parsed once so no test spells the check twice.
fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the fixture identifiers are canonical")
}

/// The resolution, whatever it answered.
fn read(triggers: &str, status: &str) -> Option<Binding> {
    resolve(triggers, status).expect(READABLE)
}

/// The resolution INCLUDING its failure arm, for the tests that expect one.
fn resolve(triggers: &str, status: &str) -> crate::error::Result<Option<Binding>> {
    Binding::read(id(FLEET), id(WORKSPACE), status, &document(triggers))
}

#[test]
fn a_fleet_with_no_webhook_trigger_resolves_to_nothing() {
    let cron = r#"[{"type":"cron","schedule":"0 9 * * *"}]"#;

    assert!(
        read(cron, FleetStatus::Active.as_str()).is_none(),
        "a cron-only fleet takes no delivery, and answering one would run it \
         on traffic its author never asked for"
    );
}

/// A fleet only ever talked to declares no trigger, and takes no delivery.
#[test]
fn a_fleet_with_no_trigger_resolves_to_nothing() {
    assert!(read("[]", FleetStatus::Active.as_str()).is_none());
}

/// The first-webhook-trigger rule, asserted where it lives.
///
/// Two triggers with different sources is a document the schema accepts, and
/// the URL carries `fleet_id` alone — so SOMETHING has to decide, and it must
/// be the same something every time. Declaration order is that rule.
#[test]
fn the_first_webhook_trigger_declared_is_the_one_that_binds() {
    let two = r#"[
        {"type":"webhook","source":"github"},
        {"type":"webhook","source":"slack"}
    ]"#;

    assert_eq!(binding_of(two).source(), "github");
}

/// A URL that names a source binds that source's trigger, wherever it sits.
///
/// The per-source route's reason to exist: on a fleet with two webhook
/// triggers the bare route can only ever reach the first, so each source needs
/// an address that reaches it by name.
#[test]
fn a_named_source_binds_its_own_trigger() {
    let two = r#"[
        {"type":"webhook","source":"linear"},
        {"type":"webhook","source":"slack"}
    ]"#;
    let named = |source| {
        Binding::stored(
            id(FLEET),
            id(WORKSPACE),
            FleetStatus::Active.as_str(),
            &document(two),
            Some(source),
        )
        .expect(READABLE)
    };

    for source in ["linear", "slack"] {
        let bound = named(source).expect("the fleet declares this source");
        assert_eq!(bound.source(), source);
    }
    assert!(
        named("github").is_none(),
        "a source the fleet does not declare binds nothing"
    );
}

/// A cron trigger ahead of the webhook must not shadow it.
///
/// The scan is for the first WEBHOOK, not the first trigger. A `find` that
/// took the head of the list would leave a fleet that schedules and receives
/// unable to receive, which is a document authors write all the time.
#[test]
fn a_leading_cron_trigger_does_not_shadow_the_webhook_behind_it() {
    let mixed = r#"[
        {"type":"cron","schedule":"0 9 * * *"},
        {"type":"webhook","source":"github"}
    ]"#;

    assert_eq!(binding_of(mixed).source(), "github");
}

#[test]
fn the_vault_key_is_the_source_when_no_override_is_authored() {
    let plain = r#"[{"type":"webhook","source":"github"}]"#;

    assert_eq!(
        binding_of(plain).credential_name(),
        "github",
        "the provider's own name IS the key when nothing overrides it"
    );
}

/// The override exists so two fleets on one provider hold different secrets.
///
/// Reading the source here instead would make both fleets share one secret,
/// and rotating either would break the other.
#[test]
fn an_authored_credential_name_overrides_the_source_as_the_vault_key() {
    let overridden = r#"[{
        "type":"webhook",
        "source":"github",
        "credential_name":"github-second-app"
    }]"#;

    let binding = binding_of(overridden);

    assert_eq!(binding.source(), "github", "the provider is unchanged");
    assert_eq!(binding.credential_name(), "github-second-app");
}

/// A source with no declared scheme must resolve to `None`, never to a
/// neighbour's.
///
/// `None` becomes `Refusal::Unconfigured` at the ingress, which is the
/// fail-closed answer. Borrowing another provider's scheme would verify a
/// delivery against the wrong canonicalisation.
#[test]
fn an_undeclared_source_resolves_to_no_scheme() {
    // Parseable only because it names a Svix secret: the Svix route verifies
    // it, and the HMAC wall must still answer "no scheme" for it.
    let jira = r#"[{"type":"webhook","source":"jira","signature":{"secret_ref":"jira_svix"}}]"#;

    assert!(binding_of(jira).scheme().is_none());
}

#[test]
fn a_declared_source_resolves_to_the_walls_own_scheme() {
    let github = r#"[{"type":"webhook","source":"github"}]"#;

    let scheme = binding_of(github).scheme().expect("github is declared");

    assert_eq!(scheme.signature_header(), "x-hub-signature-256");
}

#[test]
fn an_absent_event_list_admits_every_event() {
    let unfiltered = r#"[{"type":"webhook","source":"github"}]"#;

    let binding = binding_of(unfiltered);

    assert!(binding.admits("pull_request"));
    assert!(binding.admits("workflow_run"));
    assert!(binding.admits("anything_at_all"));
}

#[test]
fn an_authored_event_list_admits_only_what_it_names() {
    let filtered = r#"[{
        "type":"webhook",
        "source":"github",
        "events":["workflow_run"]
    }]"#;

    let binding = binding_of(filtered);

    assert!(binding.admits("workflow_run"));
    assert!(
        !binding.admits("pull_request"),
        "an event outside the list must not wake the fleet"
    );
}

/// An empty list never reaches [`Binding`], so `admits` needs no arm for one.
///
/// The schema bounds `events` at `min = 1`, which makes `Some([])`
/// unrepresentable rather than merely unusual — the document is refused at
/// parse. This pins that, because the guarantee is what lets `admits` stay a
/// single `any`: if the bound were ever relaxed, an empty list would silently
/// start denying every event, and this test is what fails first.
#[test]
fn an_empty_event_list_is_refused_at_parse_rather_than_interpreted() {
    let empty = r#"[{"type":"webhook","source":"github","events":[]}]"#;

    let refused = resolve(empty, FleetStatus::Active.as_str());

    assert!(
        refused.is_err(),
        "an empty allow-list is not a document this daemon stores, so nothing \
         downstream has to decide what one would have meant"
    );
}

/// Only `active` takes work, and the other four are not refusals here.
///
/// A paused fleet's delivery is answered 200 and dropped — the rework that
/// retired `UZ-WH-003`. This asserts the QUESTION resolves correctly; what the
/// handler does with the answer is the handler's test.
#[test]
fn only_an_active_fleet_will_take_the_delivery() {
    let github = r#"[{"type":"webhook","source":"github"}]"#;

    let runnable = [
        (FleetStatus::Installing, false),
        (FleetStatus::Active, true),
        (FleetStatus::Paused, false),
        (FleetStatus::Stopped, false),
        (FleetStatus::Killed, false),
    ];

    for (status, expected) in runnable {
        let binding = read(github, status.as_str()).expect("the trigger is declared");

        assert_eq!(
            binding.is_runnable(),
            expected,
            "`{}` must{} take work",
            status.as_str(),
            if expected { "" } else { " not" }
        );
    }
}

/// A status this build cannot name is refused, never defaulted.
///
/// A newer daemon writing a sixth status must not have it read as
/// `installing` here — this one would then act on a state it does not
/// understand.
#[test]
fn a_status_this_build_cannot_name_is_refused() {
    let github = r#"[{"type":"webhook","source":"github"}]"#;

    let refused = resolve(github, "quiesced");

    assert!(refused.is_err(), "an unknown status is not a default");
}

/// The workspace comes from the ROW, never from the request.
///
/// A signed delivery carries no principal, so nothing in the request could
/// name a workspace — and a handler that let one do so would be letting a
/// sender choose whose vault its secret is read from.
#[test]
fn the_workspace_is_the_rows_own() {
    let github = r#"[{"type":"webhook","source":"github"}]"#;

    let binding = binding_of(github);

    assert_eq!(binding.workspace().as_str(), WORKSPACE);
    assert_eq!(binding.fleet().as_str(), FLEET);
}
