//! A resident's setup, proven by reading it the way the lease does.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::config::Trigger;

use super::{ChannelId, Resident};

const TEAM: &str = "T024BE7LD";
const CHANNEL: &str = "C0123456789";

fn resident() -> Resident {
    let channel: ChannelId = CHANNEL.parse().expect("a channel identifier");
    Resident::for_channel(TEAM, &channel).expect("a Slack team forms a name")
}

/// Dimension 5.2 — the resident's policy is the one built in code: no
/// trigger, no tools, no hosts, a one-dollar ceiling. The instructions are
/// prose only, so nothing they say reaches the policy.
#[test]
fn resident_config_is_built_in_code() {
    let resident = resident();
    let config = resident.definition.runtime();

    assert_eq!(
        config.triggers(),
        &[] as &[Trigger],
        "ingress addresses a resident directly"
    );
    assert!(config.tools().is_empty(), "a resident holds no tool");
    assert!(config.network().is_none(), "a resident reaches no host");
    assert!(
        config.credentials().is_empty(),
        "a resident holds no secret"
    );
    assert!(
        config.repository_binding().is_none(),
        "a resident reaches no repository"
    );
    assert_eq!(
        config.budget().daily().nanos(),
        NANOS_PER_USD,
        "a one-dollar day"
    );
    assert_eq!(
        config.name(),
        &resident.name,
        "the configuration names the resident"
    );
    assert!(
        resident.definition.description().contains("Slack channel"),
        "the description says where the resident lives"
    );
    assert!(
        !resident
            .definition
            .instructions_md()
            .contains("x-agentsfleet"),
        "the instructions declare no policy of their own"
    );
    resident
        .definition
        .validate_for_save()
        .expect("a resident is complete enough to run");
}

/// The stored configuration the name race compares is the definition's own,
/// so a resident another mention installed is recognised as this one.
#[test]
fn the_stored_configuration_is_the_definitions() {
    let resident = resident();
    let stored: serde_json::Value =
        serde_json::from_str(&resident.config_json).expect("the stored configuration is JSON");
    let own = serde_json::to_value(resident.definition.config()).expect("the configuration");
    assert_eq!(stored, own);
}

/// The name is the team and the channel, lower-cased, so it is a fleet name and
/// the same for every mention in the channel; the instructions name the
/// channel a mention trigger would attach to.
#[test]
fn a_resident_is_named_for_its_channel() {
    let resident = resident();
    let instructions = resident.definition.instructions_md();
    assert_eq!(
        resident.name.as_str(),
        "slack-channel-t024be7ld-c0123456789"
    );
    assert!(
        instructions.contains(&format!("channel `{CHANNEL}`")),
        "{instructions}"
    );
    assert!(!instructions.contains('{'), "every placeholder is filled");
}

/// A team identifier that cannot form a fleet name forms no resident rather
/// than a name the install would refuse.
#[test]
fn a_team_that_cannot_form_a_name_forms_no_resident() {
    let channel: ChannelId = CHANNEL.parse().expect("a channel identifier");
    assert_eq!(Resident::for_channel("not a team!", &channel), None);
}
