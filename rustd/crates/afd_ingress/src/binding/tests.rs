//! What a stored row resolves to, proved with no datastore in the test.
//!
//! [`Binding::read`] takes three strings and answers, so every rule this file
//! covers — which trigger wins, which vault key it names, whether an event is
//! admitted, whether the fleet will take work — is reachable without a
//! Postgres, a Redis or a vault. That is the property worth keeping: the rules
//! a delivery is measured against must not need an environment to test, or
//! they go untested and drift.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;

use super::Binding;

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
            "budget": {{ "daily_dollars": 1.0 }}
          }}
        }}"#
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

#[test]
fn an_api_only_fleet_resolves_to_nothing() {
    let api = r#"[{"type":"api"}]"#;

    assert!(read(api, FleetStatus::Active.as_str()).is_none());
}

/// The `LIMIT 1` rule, moved out of a sub-select and now assertable.
///
/// Two triggers with different sources is a document the schema accepts, and
/// the URL carries `fleet_id` alone — so SOMETHING has to decide, and it must
/// be the same something every time. Declaration order is that rule, and it is
/// the Zig's.
#[test]
fn the_first_webhook_trigger_declared_is_the_one_that_binds() {
    let two = r#"[
        {"type":"webhook","source":"github"},
        {"type":"webhook","source":"slack"}
    ]"#;

    assert_eq!(binding_of(two).source(), "github");
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
    let jira = r#"[{"type":"webhook","source":"jira"}]"#;

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

/// The defect this reader exists to prevent, stated as a test.
///
/// A fleet declaring Slack before GitHub resolves to its SLACK trigger under
/// [`Binding::read`], because that reader takes the first webhook trigger and
/// the per-fleet URL cannot say which. On the App ingress the provider IS in
/// the URL, and measuring a GitHub delivery against a Slack trigger would read
/// the wrong allow-list and the wrong repository set — quietly, with no error
/// anywhere, because both triggers are valid.
#[test]
fn the_source_selects_the_trigger_rather_than_declaration_order() {
    let both = r#"[
        {"type":"webhook","source":"slack","events":["message"]},
        {"type":"webhook","source":"github","repositories":["owner/repo"],"events":["pull_request"]}
    ]"#;

    let first = read(both, FleetStatus::Active.as_str()).expect("a webhook trigger is declared");
    let named = read_source(both, "github").expect("a github webhook trigger is declared");

    assert_eq!(
        first.source(),
        "slack",
        "declaration order still wins when no source is named"
    );
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

#[test]
fn the_source_match_ignores_case() {
    let github = r#"[{"type":"webhook","source":"GitHub","repositories":["owner/repo"]}]"#;

    assert!(
        read_source(github, "github").is_some(),
        "an author's capitalisation is not a subscription this daemon may drop"
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
/// The Zig compares with `lower(…) = lower(…)` in SQL for this reason. A
/// case-sensitive match here would drop deliveries for a subscription an author
/// could stare straight at without seeing what was wrong.
#[test]
fn the_repository_match_ignores_case() {
    let github = r#"[{"type":"webhook","source":"github","repositories":["Owner/Repo"]}]"#;

    let binding = binding_of(github);

    assert!(binding.serves_repository("owner/repo"));
    assert!(binding.serves_repository("OWNER/REPO"));
}
