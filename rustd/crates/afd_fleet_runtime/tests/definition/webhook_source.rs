//! A webhook trigger's `source` is the last segment of the address its
//! deliveries are sent to, so it is a lowercase slug and never `approval`; and
//! a trigger must use a built-in source or declare a supported verifier.
//!
//! Driven through [`FleetConfig::authored`], the entry every reviewed setup's
//! configuration goes through.

#![expect(
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::{
    Trigger, VERIFIED_WEBHOOK_SOURCES, WEBHOOK_SOURCE_APPROVAL, WebhookVerifier,
};
use afd_validate::rules::OUTSIDE_CHARSET;
use serde_json::{Value, json};

/// What the reserved-name rule reports.
const RESERVED: &str = "`approval` is reserved for approval decisions";

/// The refusal as an author reads it: the error and every cause under it.
fn chain(refused: &afd_fleet_runtime::Error) -> String {
    std::iter::successors(Some(refused as &dyn std::error::Error), |cause| {
        cause.source()
    })
    .map(ToString::to_string)
    .collect::<Vec<_>>()
    .join(": ")
}

/// A configuration declaring `triggers` verbatim.
fn config_of(triggers: &Value) -> String {
    json!({"name": "webhook-source-probe", "x-agentsfleet": {
        "triggers": triggers,
        "tools": [],
        "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
    }})
    .to_string()
}

/// A configuration declaring one webhook trigger and its optional verifier.
fn document_with(source: &str, verifier: Option<Value>) -> String {
    let mut trigger = json!({"type": "webhook", "source": source});
    if let Some(verifier) = verifier {
        trigger
            .as_object_mut()
            .unwrap_or_else(|| panic!("a trigger is an object"))
            .insert("verifier".to_owned(), verifier);
    }
    config_of(&json!([trigger]))
}

/// A configuration declaring one unsigned webhook trigger from `source`.
fn document(source: &str) -> String {
    document_with(source, None)
}

/// Every source whose scheme this daemon ships parses bare and keeps its text.
#[test]
fn a_verified_source_is_accepted_without_a_signature() {
    for source in VERIFIED_WEBHOOK_SOURCES {
        let config = FleetConfig::authored(&document(source))
            .unwrap_or_else(|refused| panic!("`{source}` is verifiable: {}", chain(&refused)));
        assert!(
            matches!(config.triggers(), [Trigger::Webhook(hook)] if &*hook.source == source),
            "`{source}` keeps its text"
        );
    }
}

/// Custom sources declare a closed recipe; registry sources cannot override it.
#[test]
fn an_unverifiable_trigger_never_parses() {
    let Err(refused) = FleetConfig::authored(&document("acme")) else {
        panic!("unverifiable")
    };
    let sentence = chain(&refused);
    for family in afd_webhook::Family::ALL {
        assert!(sentence.contains(family.token()), "{sentence}");
    }
    let unknown = document_with("acme", Some(json!({"scheme":"sha512-hex"})));
    let Err(refused) = FleetConfig::authored(&unknown) else {
        panic!("unknown recipe")
    };
    for family in afd_webhook::Family::ALL {
        assert!(chain(&refused).contains(family.token()));
    }
}

#[test]
fn a_verifier_that_cannot_hold_is_refused_at_parse() {
    for (source, declaration) in [
        ("github", json!({"scheme":"svix-v1"})),
        ("acme", json!({"scheme":"slack-v0","header":"x-proof"})),
        ("acme", json!({"scheme":"svix-v1","header":"x-proof"})),
        ("acme", json!({"scheme":"body-hex","header":"x acme"})),
        ("acme", json!({"scheme":"body-hex","header":"x\r\nproof"})),
        (
            "acme",
            json!({"scheme":"body-hex","header":"x".repeat(129)}),
        ),
        ("acme", json!({"scheme":"body-hex"})),
    ] {
        assert!(
            FleetConfig::authored(&document_with(source, Some(declaration.clone()))).is_err(),
            "{source}: {declaration}"
        );
    }
}

#[test]
fn every_recipe_parses_and_keeps_its_header_policy() {
    for family in afd_webhook::Family::ALL {
        let declaration = match family {
            afd_webhook::Family::SlackV0 | afd_webhook::Family::SvixV1 => {
                json!({"scheme":family.token()})
            }
            _ => json!({"scheme":family.token(),"header":"X-Acme-Proof"}),
        };
        let config = FleetConfig::authored(&document_with("acme", Some(declaration)))
            .unwrap_or_else(|refused| panic!("{}: {}", family.token(), chain(&refused)));
        assert!(matches!(config.triggers(), [Trigger::Webhook(hook)]
            if hook.verifier.as_ref().map(WebhookVerifier::family) == Some(*family)));
    }
}

/// Mention triggers share the `source` key but not the webhook rule: a chat
/// provider this daemon ships no webhook scheme for is still a mention source.
#[test]
fn a_mention_source_is_not_held_to_the_webhook_rule() {
    let mention = json!([{"type": "mention", "source": "teams", "channels": ["C0123456789"]}]);
    FleetConfig::authored(&config_of(&mention))
        .unwrap_or_else(|refused| panic!("a mention source: {}", chain(&refused)));
}

/// `approval` is the approval-decision route's segment; any character outside
/// the slug charset, including case, would be an address no route serves.
#[test]
fn a_reserved_or_non_slug_source_is_refused_with_its_rule() {
    for (source, rule) in [
        (WEBHOOK_SOURCE_APPROVAL, RESERVED),
        ("A/b", OUTSIDE_CHARSET),
        ("Linear", OUTSIDE_CHARSET),
        ("pager duty", OUTSIDE_CHARSET),
    ] {
        let Err(refused) = FleetConfig::authored(&document(source)) else {
            panic!("`{source}` is refused");
        };
        let chain = chain(&refused);
        assert!(chain.contains(rule), "`{source}` names `{rule}`: {chain}");
    }
}
