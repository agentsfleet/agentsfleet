//! A webhook trigger's `source` is the last segment of the address its
//! deliveries are sent to, so it is a lowercase slug and never `approval`; and
//! a trigger must be one some route can verify — a source whose scheme this
//! daemon ships, or a Svix `signature.secret_ref`.
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
    Trigger, VERIFIED_WEBHOOK_SOURCES, WEBHOOK_SOURCE_APPROVAL, WebhookSignature,
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

/// The Svix secret the signed cases declare.
const SVIX_SECRET: &str = "stripe_svix";

/// A configuration declaring `triggers` verbatim.
fn config_of(triggers: &Value) -> String {
    json!({"name": "webhook-source-probe", "x-agentsfleet": {
        "triggers": triggers,
        "tools": [],
        "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
    }})
    .to_string()
}

/// A configuration declaring one webhook trigger from `source`, signed for the
/// Svix route when `signature` is given.
fn document_with(source: &str, signature: Option<Value>) -> String {
    let mut trigger = json!({"type": "webhook", "source": source});
    if let Some(signature) = signature {
        trigger
            .as_object_mut()
            .unwrap_or_else(|| panic!("a trigger is an object"))
            .insert("signature".to_owned(), signature);
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

/// A source no scheme is shipped for is refused bare, with a sentence naming
/// every verifiable source, and accepted once it names a Svix secret.
#[test]
fn an_unverifiable_source_needs_a_svix_secret() {
    let named = VERIFIED_WEBHOOK_SOURCES
        .map(|source| format!("`{source}`"))
        .join(", ");
    let expected = format!(
        "the webhook source `stripe` cannot be verified: use {named}, or declare \
         `signature.secret_ref` for a Svix-signed source"
    );
    let Err(refused) = FleetConfig::authored(&document("stripe")) else {
        panic!("a bare `stripe` trigger is refused");
    };
    assert!(chain(&refused).contains(&expected), "{}", chain(&refused));

    for source in ["stripe", "pager-duty2"] {
        let signed = document_with(source, Some(json!({"secret_ref": SVIX_SECRET})));
        let config = FleetConfig::authored(&signed)
            .unwrap_or_else(|refused| panic!("`{source}` with a secret: {}", chain(&refused)));
        assert!(
            matches!(
                config.triggers(),
                [Trigger::Webhook(hook)]
                    if hook.signature.as_ref().map(WebhookSignature::secret_ref)
                        == Some(SVIX_SECRET)
            ),
            "`{source}` keeps its Svix secret"
        );
    }
}

/// The header, prefix and timestamp keys were never read by any route, so a
/// signature block naming one is refused by name.
#[test]
fn a_signature_names_only_its_secret() {
    for key in ["header", "prefix", "ts_header"] {
        let signature = json!({"secret_ref": SVIX_SECRET, key: "x-signature"});
        let Err(refused) = FleetConfig::authored(&document_with("stripe", Some(signature))) else {
            panic!("a signature naming `{key}` is refused");
        };
        assert!(
            chain(&refused).contains(&format!("unknown field `{key}`")),
            "{}",
            chain(&refused)
        );
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
