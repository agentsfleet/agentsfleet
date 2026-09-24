//! A resident's documents, proven by parsing them the way an install does.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_fleet_runtime::config::Trigger;

use super::{ChannelId, Resident};

const TEAM: &str = "T024BE7LD";
const CHANNEL: &str = "C0123456789";

fn resident() -> Resident {
    let channel: ChannelId = CHANNEL.parse().expect("a channel identifier");
    Resident::for_channel(TEAM, &channel).expect("a Slack team forms a name")
}

/// Dimension 5.2 — the resident's policy is the one built in code: one `api`
/// trigger, no tools, no hosts, a one-dollar ceiling. The skill carries a name
/// and prose only, so nothing it says reaches the policy.
#[test]
fn resident_config_is_built_in_code() {
    let resident = resident();
    let parsed =
        afd_fleet_runtime::parse_trigger(&resident.trigger_markdown).expect("the policy parses");
    let config = parsed.config();

    assert_eq!(config.triggers(), [Trigger::Api]);
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
    let daily = config.budget().daily().dollars();
    assert!(
        (daily - 1.0).abs() < f64::EPSILON,
        "a one-dollar day: {daily}"
    );

    let skill = afd_fleet_runtime::parse_skill(&resident.skill_markdown).expect("the skill parses");
    assert_eq!(skill.name(), config.name(), "one bundle, one identity");
    assert!(
        !resident.skill_markdown.contains("x-agentsfleet"),
        "the skill declares no policy of its own"
    );
}

/// The name is the team and the channel, lower-cased, so it is a fleet name and
/// the same for every mention in the channel; the skill names the channel in
/// the attach command it offers.
#[test]
fn a_resident_is_named_for_its_channel() {
    let resident = resident();
    assert_eq!(
        resident.name.as_str(),
        "slack-channel-t024be7ld-c0123456789"
    );
    assert!(
        resident
            .skill_markdown
            .contains(&format!("--slack-channel {CHANNEL}")),
        "{}",
        resident.skill_markdown
    );
    assert!(
        !resident.skill_markdown.contains('{'),
        "every placeholder is filled"
    );
}

/// A team identifier that cannot form a fleet name forms no resident rather
/// than a name the install would refuse.
#[test]
fn a_team_that_cannot_form_a_name_forms_no_resident() {
    let channel: ChannelId = CHANNEL.parse().expect("a channel identifier");
    assert_eq!(Resident::for_channel("not a team!", &channel), None);
}
