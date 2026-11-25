//! What a fleet subscribed to a channel is, as routing reads it — decided
//! from its row alone, so every rule is proven with no datastore.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use af_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use serde_json::json;

use super::{Subscriber, subscribed};

/// The fleet every case reads.
const FLEET: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0120";

fn fleet() -> Uuid7 {
    Uuid7::parse(FLEET).expect("a v7 id")
}

/// The GitHub scope a person approved, as the grant stores it.
fn scope(access: &str) -> String {
    // A write binding opens a Pull Request, so it names a base; a read one
    // opens nothing and may not.
    let scope = if access == "write" {
        json!({"repositories": ["acme/widgets"], "access": access, "base": "main"})
    } else {
        json!({"repositories": ["acme/widgets"], "access": access})
    };
    scope.to_string()
}

/// What routing needs from each subscriber: its saved name, whether it can
/// run, and whether it takes only mentions addressed to it.
#[test]
fn a_subscriber_carries_its_name_status_and_reach() {
    let read_bound = subscribed(
        fleet(),
        "responder",
        FleetStatus::Active.as_str(),
        Some(&scope("read")),
    )
    .expect("a readable row");
    assert_eq!(
        read_bound,
        Subscriber {
            fleet: fleet(),
            name: "responder".to_owned(),
            runnable: true,
            addressed_only: false,
        }
    );

    let write_bound = subscribed(
        fleet(),
        "repairer",
        FleetStatus::Archived.as_str(),
        Some(&scope("write")),
    )
    .expect("a readable row");
    assert!(!write_bound.runnable, "an archived fleet cannot run now");
    assert!(
        write_bound.addressed_only,
        "a fleet a person approved to write never takes an unaddressed mention"
    );
}

/// A fleet holding no approved GitHub scope may answer any mention in its
/// channel: it can push nowhere.
#[test]
fn a_fleet_with_no_github_grant_takes_unaddressed_mentions() {
    let subscriber = subscribed(fleet(), "incident", FleetStatus::Active.as_str(), None)
        .expect("a readable row");

    assert!(!subscriber.addressed_only);
}

/// A status this build cannot name and a stored scope the parser refuses are
/// refused, not read as an unaddressed fleet.
#[test]
fn an_unreadable_status_or_scope_is_refused() {
    assert!(
        subscribed(fleet(), "incident", "installing", None).is_err(),
        "a status this build cannot name is an incident"
    );
    assert!(
        subscribed(
            fleet(),
            "incident",
            FleetStatus::Active.as_str(),
            Some(r#"{"repositories": ["acme/widgets"]}"#),
        )
        .is_err(),
        "a half-declared scope is a row nothing writes, never a read reach"
    );
}
