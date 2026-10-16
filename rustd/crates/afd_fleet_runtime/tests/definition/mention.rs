//! The `mention` trigger: a fleet attached to exactly one chat channel, by the
//! identifier its provider minted.
//!
//! Driven through [`FleetConfig::authored`], the one entry a reviewed setup's
//! configuration goes through, because the rule is split across two layers on
//! purpose: the schema counts the `channels` entries and names the key when the
//! count is wrong, and the typed layer checks each identifier's shape once. A
//! test of either layer alone would pass while the other drifted.

#![expect(
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Trigger;
use serde_json::{Value, json};

/// A public channel's identifier, in the shape Slack mints.
const PUBLIC_CHANNEL: &str = "C0123456789";
/// A private channel's identifier.
const PRIVATE_CHANNEL: &str = "G0123456789";
/// The provider a mention trigger routes through.
const SOURCE: &str = "slack";

/// The key every channel refusal must name, so an author can find the line.
const CHANNELS_KEY: &str = "channels";

/// The refusal as an author reads it: the error and every cause under it.
///
/// A schema bound keeps `garde`'s report as its source, which is where the
/// refused PATH lives; the top line alone names only the class.
fn chain(refused: &afd_fleet_runtime::Error) -> String {
    std::iter::successors(Some(refused as &dyn std::error::Error), |cause| {
        cause.source()
    })
    .map(ToString::to_string)
    .collect::<Vec<_>>()
    .join(": ")
}

/// A configuration declaring `triggers` verbatim.
fn document(triggers: &Value) -> String {
    json!({"name": "mention-probe", "x-agentsfleet": {
        "triggers": triggers, "tools": [], "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
    }})
    .to_string()
}

/// One mention trigger, with `channels` set only when given.
fn mention(channels: Option<Value>) -> Value {
    let mut trigger = json!({"type": "mention", "source": SOURCE});
    if let Some(channels) = channels {
        trigger
            .as_object_mut()
            .expect("a trigger is an object")
            .insert(CHANNELS_KEY.to_owned(), channels);
    }
    json!([trigger])
}

/// One well-formed channel parses into the typed trigger and attaches the
/// fleet to exactly that channel; no channel, two, a lower-case identifier, a
/// direct message's and a short one are each refused naming the key.
#[test]
fn mention_trigger_takes_exactly_one_channel_id() {
    for channel in [PUBLIC_CHANNEL, PRIVATE_CHANNEL] {
        let parsed = FleetConfig::authored(&document(&mention(Some(json!([channel])))))
            .unwrap_or_else(|failure| panic!("{channel} is a usable channel: {failure}"));
        let mentions: Vec<&str> = parsed
            .triggers()
            .iter()
            .filter_map(|trigger| match trigger {
                Trigger::Mention(mention) => Some(mention.channel.as_str()),
                Trigger::Webhook(_) | Trigger::Cron(_) => None,
            })
            .collect();
        assert_eq!(
            mentions,
            vec![channel],
            "{channel} is the one channel attached"
        );
        let attached = channel
            .parse()
            .unwrap_or_else(|_bad| panic!("{channel} is an identifier"));
        assert!(
            parsed.is_attached_to("Slack", &attached),
            "the provider compares without case"
        );
        let other = if channel == PUBLIC_CHANNEL {
            PRIVATE_CHANNEL
        } else {
            PUBLIC_CHANNEL
        };
        let unattached = other
            .parse()
            .unwrap_or_else(|_bad| panic!("{other} is an identifier"));
        assert!(
            !parsed.is_attached_to(SOURCE, &unattached),
            "{other} is a second audience"
        );
    }

    for (case, channels) in [
        ("no channels key", None),
        ("an empty list", Some(json!([]))),
        (
            "two channels",
            Some(json!([PUBLIC_CHANNEL, PRIVATE_CHANNEL])),
        ),
        ("a lower-case identifier", Some(json!(["c0123456789"]))),
        ("a direct message", Some(json!(["D0123456789"]))),
        ("a short identifier", Some(json!(["C0123"]))),
        ("a name, not an identifier", Some(json!(["ci-dev"]))),
    ] {
        let Err(refused) = FleetConfig::authored(&document(&mention(channels))) else {
            panic!("{case}: a mention trigger without exactly one channel identifier is refused");
        };
        let reported = chain(&refused);
        assert!(
            reported.contains(CHANNELS_KEY),
            "{case}: the refusal names `{CHANNELS_KEY}`: {reported}"
        );
    }
}

/// One channel per fleet: a second mention trigger is a second audience, so it
/// is refused as a set rather than silently merged.
#[test]
fn a_fleet_attaches_to_one_channel_only() {
    let triggers = json!([
        {"type": "mention", "source": SOURCE, CHANNELS_KEY: [PUBLIC_CHANNEL]},
        {"type": "mention", "source": SOURCE, CHANNELS_KEY: [PRIVATE_CHANNEL]}
    ]);
    let Err(refused) = FleetConfig::authored(&document(&triggers)) else {
        panic!("two mention triggers are refused");
    };
    assert!(
        refused.to_string().contains("only one channel"),
        "the refusal names the rule: {refused}"
    );
}

/// A mention trigger with no source names no provider to route it through.
#[test]
fn a_mention_trigger_names_its_source() {
    let triggers = json!([{"type": "mention", CHANNELS_KEY: [PUBLIC_CHANNEL]}]);
    let Err(refused) = FleetConfig::authored(&document(&triggers)) else {
        panic!("a mention trigger with no source is refused");
    };
    assert!(
        refused.to_string().contains("names no source"),
        "the refusal names the rule: {refused}"
    );
}
