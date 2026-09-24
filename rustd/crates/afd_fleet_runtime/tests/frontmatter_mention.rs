//! The `mention` trigger: a fleet attached to exactly one chat channel, by the
//! identifier its provider minted.
//!
//! Driven through `parse_trigger`, the one public entry a stored `TRIGGER.md`
//! goes through, because the rule is split across two layers on purpose: the
//! schema counts the `channels` entries and names the key when the count is
//! wrong, and the typed layer checks each identifier's shape once. A test of
//! either layer alone would pass while the other drifted.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet_runtime::config::Trigger;
use afd_fleet_runtime::parse_trigger;

/// A public channel's identifier, in the shape Slack mints.
const PUBLIC_CHANNEL: &str = "C0123456789";
/// A private channel's identifier.
const PRIVATE_CHANNEL: &str = "G0123456789";

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

/// A document declaring one mention trigger with `channels` rendered verbatim.
fn document(channels: &str) -> String {
    format!(
        "---\nname: mention-probe\nx-agentsfleet:\n  triggers:\n    - type: mention\n      \
         source: slack\n{channels}  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n"
    )
}

/// Dimension 2.1 — one well-formed channel parses into the typed trigger; no
/// channel, two, a lower-case identifier, a direct message's and a short one
/// are each refused naming the key.
#[test]
fn mention_trigger_takes_exactly_one_channel_id() {
    for channel in [PUBLIC_CHANNEL, PRIVATE_CHANNEL] {
        let parsed = parse_trigger(&document(&format!("      channels: [{channel}]\n")))
            .expect("one well-formed channel is a usable document");
        let mentions: Vec<&str> = parsed
            .config()
            .triggers()
            .iter()
            .filter_map(|trigger| match trigger {
                Trigger::Mention(mention) => Some(mention.channel.as_str()),
                Trigger::Webhook(_) | Trigger::Cron(_) | Trigger::Api => None,
            })
            .collect();
        assert_eq!(
            mentions,
            vec![channel],
            "{channel} is the one channel attached"
        );
    }

    for (case, channels) in [
        ("no channels key", String::new()),
        ("an empty list", "      channels: []\n".to_owned()),
        (
            "two channels",
            format!("      channels: [{PUBLIC_CHANNEL}, {PRIVATE_CHANNEL}]\n"),
        ),
        (
            "a lower-case identifier",
            "      channels: [c0123456789]\n".to_owned(),
        ),
        (
            "a direct message",
            "      channels: [D0123456789]\n".to_owned(),
        ),
        ("a short identifier", "      channels: [C0123]\n".to_owned()),
        (
            "a name, not an identifier",
            "      channels: [ci-dev]\n".to_owned(),
        ),
    ] {
        let refused = parse_trigger(&document(&channels))
            .expect_err("a mention trigger without exactly one channel identifier is refused");
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
    let document = format!(
        "---\nname: mention-probe\nx-agentsfleet:\n  triggers:\n    - type: mention\n      \
         source: slack\n      channels: [{PUBLIC_CHANNEL}]\n    - type: mention\n      \
         source: slack\n      channels: [{PRIVATE_CHANNEL}]\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n"
    );
    let refused = parse_trigger(&document).expect_err("two mention triggers are refused");
    assert!(
        refused.to_string().contains("only one channel"),
        "the refusal names the rule: {refused}"
    );
}

/// A mention trigger with no source names no provider to route it through.
#[test]
fn a_mention_trigger_names_its_source() {
    let document = format!(
        "---\nname: mention-probe\nx-agentsfleet:\n  triggers:\n    - type: mention\n      \
         channels: [{PUBLIC_CHANNEL}]\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n"
    );
    let refused =
        parse_trigger(&document).expect_err("a mention trigger with no source is refused");
    assert!(
        refused.to_string().contains("names no source"),
        "the refusal names the rule: {refused}"
    );
}
