//! Which stored fleets a mention in one channel reaches — decided from the
//! document alone, so every rule is proven with no datastore.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_runtime::config::ChannelId;

use super::{Subscriber, subscribed};

/// The provider a mention arrives from.
const SLACK: &str = "slack";
/// The channel under test, and a second one.
const CHANNEL: &str = "C0123456789";
const OTHER_CHANNEL: &str = "C0987654321";
/// The fleet every case reads.
const FLEET: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0120";

fn channel(id: &str) -> ChannelId {
    id.parse().expect("a well-formed channel identifier")
}

fn fleet() -> Uuid7 {
    Uuid7::parse(FLEET).expect("a v7 id")
}

/// The JSON an install stores for a fleet attached to `channel`, optionally
/// bound to a repository with `access`.
fn stored(name: &str, channel: &str, access: Option<&str>) -> String {
    // A write binding opens a Pull Request, so it names a base; a read one
    // opens nothing and may not.
    let binding = access.map_or_else(String::new, |access| {
        let base = if access == "write" {
            "  repository_base: main\n"
        } else {
            ""
        };
        format!("  repositories: [acme/widgets]\n  repository_access: {access}\n{base}")
    });
    let markdown = format!(
        "---\nname: {name}\nx-agentsfleet:\n  triggers:\n    - type: mention\n      \
         source: {SLACK}\n      channels: [{channel}]\n  tools: []\n  budget:\n    \
         daily_dollars: 1.0\n{binding}---\n"
    );
    afd_fleet_runtime::parse_trigger(&markdown)
        .expect("the probe document parses")
        .config_json()
        .to_owned()
}

/// Dimension 2.3 — the subscription is the channel's identifier: the document
/// names no channel NAME, so renaming the channel in Slack changes nothing,
/// and editing the identifier moves the fleet from one channel to the other.
#[test]
fn subscription_follows_the_channel_id() {
    let active = FleetStatus::Active.as_str();
    let before = stored("incident", CHANNEL, None);
    let after = stored("incident", OTHER_CHANNEL, None);

    let reached = |document: &str, id: &str| {
        subscribed(fleet(), active, document, SLACK, &channel(id))
            .expect("a readable row")
            .is_some()
    };
    assert!(
        reached(&before, CHANNEL),
        "attached where the document says"
    );
    assert!(!reached(&before, OTHER_CHANNEL), "and nowhere else");
    assert!(
        reached(&after, OTHER_CHANNEL),
        "the edited identifier moves it"
    );
    assert!(!reached(&after, CHANNEL), "and it leaves the old channel");
}

/// What routing needs from each subscriber: its name, whether it can run, and
/// whether it takes only mentions addressed to it.
#[test]
fn a_subscriber_carries_its_name_status_and_reach() {
    let read_bound = subscribed(
        fleet(),
        FleetStatus::Active.as_str(),
        &stored("responder", CHANNEL, Some("read")),
        SLACK,
        &channel(CHANNEL),
    )
    .expect("a readable row");
    assert_eq!(
        read_bound,
        Some(Subscriber {
            fleet: fleet(),
            name: "responder".to_owned(),
            runnable: true,
            addressed_only: false,
        })
    );

    let write_bound = subscribed(
        fleet(),
        FleetStatus::Paused.as_str(),
        &stored("repairer", CHANNEL, Some("write")),
        SLACK,
        &channel(CHANNEL),
    )
    .expect("a readable row")
    .expect("attached to the channel");
    assert!(!write_bound.runnable, "a paused fleet cannot run now");
    assert!(
        write_bound.addressed_only,
        "a fleet that can write never takes an unaddressed mention"
    );
}

/// The provider is compared ignoring case, as a webhook source is; another
/// provider's mention trigger does not reach this one.
#[test]
fn the_provider_must_match_ignoring_case() {
    let document = stored("incident", CHANNEL, None);
    let active = FleetStatus::Active.as_str();
    let on = |provider: &str| {
        subscribed(fleet(), active, &document, provider, &channel(CHANNEL))
            .expect("a readable row")
            .is_some()
    };
    assert!(on("SLACK"), "the spelling's case is nobody's authority");
    assert!(
        !on("discord"),
        "another provider's channel is another channel"
    );
}

/// A fleet with no mention trigger is not attached; a status this build
/// cannot name and a document that no longer parses are refused, not skipped.
#[test]
fn unattached_and_unreadable_rows_are_told_apart() {
    let api_only = afd_fleet_runtime::parse_trigger(
        "---\nname: api-only\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  \
         budget:\n    daily_dollars: 1.0\n---\n",
    )
    .expect("the probe document parses")
    .config_json()
    .to_owned();
    assert_eq!(
        subscribed(
            fleet(),
            FleetStatus::Active.as_str(),
            &api_only,
            SLACK,
            &channel(CHANNEL)
        )
        .expect("a readable row"),
        None
    );

    let document = stored("incident", CHANNEL, None);
    assert!(
        subscribed(fleet(), "hibernating", &document, SLACK, &channel(CHANNEL)).is_err(),
        "a status this build cannot name is an incident, not an unattached fleet"
    );
    assert!(
        subscribed(
            fleet(),
            FleetStatus::Active.as_str(),
            "not json",
            SLACK,
            &channel(CHANNEL)
        )
        .is_err(),
        "a document that no longer parses is an incident too"
    );
}
