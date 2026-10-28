//! The corpus's field VALUES, read through the shared Definition parser.
//!
//! `corpus.rs` asserts which documents open and which are refused. This one
//! takes the documents that DO open and checks that every authored value
//! survives — a document that parses into the wrong policy is a break the
//! verdict table cannot see.

#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Trigger;
use serde_json::json;

use crate::support::{CONTEXT_CAP_VALUE, MODEL_VALUE, definition, definition_path};

/// `minimal.json` carries the cron trigger, tool and ceiling it declares.
#[test]
fn the_minimal_definition_carries_its_authored_values() {
    let parsed = definition("definition/minimal.json");
    let config = parsed.runtime();

    assert_eq!(config.name().as_str(), "minimal-setup");
    assert_eq!(
        config
            .tools()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["agentmail"]
    );
    assert_eq!(config.budget().daily().nanos(), NANOS_PER_USD);
    assert!(matches!(
        config.triggers(),
        [Trigger::Cron(cron)] if &*cron.schedule == "0 9 * * *"
    ));
}

/// `full.json` carries its webhook signature, credentials, both ceilings and
/// its description.
#[test]
fn the_full_definition_carries_every_authored_block() {
    let parsed = definition("definition/full.json");
    let config = parsed.runtime();

    assert_eq!(config.name().as_str(), "full-setup");
    assert_eq!(config.tools().len(), 3);
    assert_eq!(config.credentials().len(), 2);
    assert!(config.budget().monthly().is_some());
    assert_eq!(
        config.network().map(|network| network.allow().len()),
        Some(2)
    );
    let [Trigger::Webhook(hook)] = config.triggers() else {
        panic!("full.json declares exactly one webhook trigger");
    };
    assert_eq!(&*hook.source, "github");
    assert_eq!(hook.credential_name.as_deref(), Some("github_secret"));

    assert_eq!(
        parsed.description(),
        "Every optional configuration block populated."
    );
}

/// `"tool_window": "auto"` resolves to the runner's zero sentinel, not to a
/// refusal: the one knob that takes a WORD where its siblings take a number.
#[test]
fn an_auto_tool_window_resolves_to_the_zero_sentinel() {
    let parsed = definition("definition/with_model_and_context.json");
    let config = parsed.runtime();
    let Some(context) = config.context() else {
        panic!("with_model_and_context.json declares a context block");
    };

    assert_eq!(config.model(), Some(MODEL_VALUE));
    assert_eq!(context.context_cap_tokens, CONTEXT_CAP_VALUE);
    assert_eq!(context.tool_window, 0);
    assert_eq!(context.memory_checkpoint_every, 5);
    assert!((context.stage_chunk_threshold - 0.75).abs() < f32::EPSILON);
}

/// The declarative-schedule fleet carries its cron fields and no local timer.
#[test]
fn the_declarative_schedule_definition_carries_its_cron_fields() {
    let parsed = definition(&definition_path("zoho-sprint-daily-summarizer"));

    let [Trigger::Cron(cron)] = parsed.runtime().triggers() else {
        panic!("the summarizer declares exactly one cron trigger");
    };
    assert_eq!(&*cron.schedule, "0 9 * * *");
    assert_eq!(&*cron.timezone, "Asia/Kolkata");
    assert_eq!(&*cron.message, "Summarize today's Zoho Sprints activity");
}

/// A configuration whose single gate rule declares `kind`.
fn declaring_gate_kind(kind: &str) -> String {
    json!({"name": "gate-kind-probe", "x-agentsfleet": {
        "triggers": [], "tools": [],
        "budget": {"daily_nanos": NANOS_PER_USD.to_string()},
        "gates": {"rules": [{"tool": "shell", "action": "run", "gate_kind": kind}]}
    }})
    .to_string()
}

/// A fleet may not author the daemon's own gate kind.
///
/// The wiring proof, separate from the validator's own unit test: `gate_kind`
/// sits inside `Option<Vec<GateRule>>` and is only reached because
/// `Gates::rules` carries `#[garde(dive)]`, so a lost `dive` would leave the
/// guard present and inert. The kind travels verbatim onto the approval card,
/// and `afd_approval`'s resolve moves a credential grant for cards of this
/// kind, so a fleet that could spell it could talk an operator into a grant.
#[test]
fn a_fleet_may_not_author_a_daemon_owned_gate_kind() {
    assert!(
        FleetConfig::authored(&declaring_gate_kind("integration_grant")).is_err(),
        "the reserved kind must be refused through parse, not merely by the validator"
    );
}

/// The retired write kind parses like any other kind a fleet authors, and the
/// same document with an ordinary kind parses too, so the refusal above is the
/// guard rather than an unrelated defect in the document.
#[test]
fn ordinary_and_retired_gate_kinds_are_a_fleets_own_business() {
    for kind in ["repository_write", "destructive_action"] {
        let parsed = FleetConfig::authored(&declaring_gate_kind(kind))
            .unwrap_or_else(|failure| panic!("{kind} should parse: {failure}"));
        assert_eq!(parsed.name().as_str(), "gate-kind-probe");
    }
}

/// A key this build does not know inside `context` is REFUSED by name, not
/// ignored: the block is a budget, and a dropped knob would run the fleet
/// under limits its author did not write.
#[test]
fn an_unknown_key_inside_the_context_block_is_refused_by_name() {
    const UNKNOWN_KEY: &str = "context_cap_tokns";
    let document = json!({"name": "unknown-context-knob", "x-agentsfleet": {
        "context": {UNKNOWN_KEY: CONTEXT_CAP_VALUE},
        "triggers": [{"type": "webhook", "source": "github"}],
        "tools": ["http_request"],
        "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
    }});

    let Err(refused) = FleetConfig::authored(&document.to_string()) else {
        panic!("an unknown context knob is refused");
    };
    let rendered = refused.to_string();
    assert!(
        rendered.contains(UNKNOWN_KEY),
        "the refusal must name the field an author can fix: {rendered}"
    );
}
