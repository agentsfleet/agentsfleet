//! The trigger kinds a setup may declare, and a set that declares none.
//!
//! A fleet is woken by the messages sent to it whatever it declares, so the
//! trigger set says only what ELSE may wake it, and an empty one is a fleet
//! that is only ever talked to.

#![expect(
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet_runtime::config::{KIND_CRON, KIND_MENTION, KIND_WEBHOOK, TRIGGER_KINDS};
use afd_fleet_runtime::definition::canonical_digest;
use afd_fleet_runtime::{Definition, FleetConfig};
use serde_json::{Value, json};

use super::{parse, value};

/// The runtime block's key in a configuration document.
const RUNTIME_BLOCK: &str = "x-agentsfleet";
/// Where the shared Definition fixture keeps its runtime block.
const RUNTIME_POINTER: &str = "/config/x-agentsfleet";
/// The trigger set's key inside it.
const TRIGGERS: &str = "triggers";

/// The refusal as an author reads it: the error and every cause under it.
fn chain(refused: &afd_fleet_runtime::Error) -> String {
    std::iter::successors(Some(refused as &dyn std::error::Error), |cause| {
        cause.source()
    })
    .map(ToString::to_string)
    .collect::<Vec<_>>()
    .join(": ")
}

/// The fixture's configuration with its trigger set replaced, or removed when
/// `triggers` is `None`.
fn config_with(triggers: Option<Value>) -> Value {
    let mut document = value();
    let block = document
        .pointer_mut(RUNTIME_POINTER)
        .and_then(Value::as_object_mut)
        .expect("the fixture carries a runtime block");
    match triggers {
        Some(set) => block.insert(TRIGGERS.to_owned(), set),
        None => block.remove(TRIGGERS),
    };
    document
}

/// The smallest trigger of `kind` the parser accepts.
fn smallest(kind: &str) -> Value {
    match kind {
        KIND_WEBHOOK => json!({"type": KIND_WEBHOOK, "source": "linear"}),
        KIND_CRON => json!({"type": KIND_CRON, "schedule": "0 9 * * *"}),
        KIND_MENTION => {
            json!({"type": KIND_MENTION, "source": "slack", "channels": ["C0123456789"]})
        }
        other => panic!("`{other}` is listed in TRIGGER_KINDS but has no example here"),
    }
}

/// Every kind the constants name parses, and serde's refusal of anything else
/// names exactly those kinds — so a kind added to the parser and not to
/// `TRIGGER_KINDS` (which the authoring prompt is built from) fails here.
#[test]
fn every_trigger_kind_is_one_the_parser_accepts() {
    for kind in TRIGGER_KINDS {
        let document = config_with(Some(json!([smallest(kind)])));
        Definition::parse(&document.to_string())
            .unwrap_or_else(|refused| panic!("`{kind}` parses: {}", chain(&refused)));
    }

    let named = TRIGGER_KINDS.map(|kind| format!("`{kind}`")).join(", ");
    let document = config_with(Some(json!([{"type": "api"}])));
    let refused = Definition::parse(&document.to_string()).expect_err("`api` is not a trigger");
    // The refused spelling is the document's own value, so it is not echoed;
    // the accepted spellings are the parser's, so they are.
    let chain = chain(&refused);
    assert!(
        chain.contains(&format!("unknown variant, expected one of {named}")),
        "the refusal names the kinds a setup may declare: {chain}"
    );
}

/// A set left out and a set left empty are one fleet: both parse with no
/// trigger, store `triggers: []`, and hash to one digest.
#[test]
fn an_absent_and_an_empty_trigger_set_are_one_message_only_fleet() {
    let absent = parse(&config_with(None));
    let empty = parse(&config_with(Some(json!([]))));

    for definition in [&absent, &empty] {
        assert!(
            definition.runtime().triggers().is_empty(),
            "no trigger is declared"
        );
        assert_eq!(
            definition
                .config()
                .value()
                .pointer(&format!("/{RUNTIME_BLOCK}/{TRIGGERS}")),
            Some(&json!([])),
            "the stored document spells the empty set"
        );
    }
    assert_eq!(
        canonical_digest(&absent).expect("digest"),
        canonical_digest(&empty).expect("digest")
    );

    let stored = json!({
        "name": "message-only",
        // pin test: literal is the contract
        RUNTIME_BLOCK: {"tools": [], "budget": {"daily_nanos": "1000000000"}}
    });
    let config =
        FleetConfig::stored(&stored.to_string()).expect("no trigger set is a valid config");
    assert!(
        config.triggers().is_empty(),
        "an absent set is the empty set"
    );
}
