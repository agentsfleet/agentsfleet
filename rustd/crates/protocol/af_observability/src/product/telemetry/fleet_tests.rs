//! The funnel's two steps: their names, their attribution, and the exact
//! property set each one may carry.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use af_core::id::Uuid7;

use super::{AccessChange, FleetOrigin, FleetStep, Telemetry};

/// The person a fixture step is attributed to.
const ACTOR: &str = "user_2funnel";

fn fleet() -> Uuid7 {
    Uuid7::parse("01924f4e-0000-7000-8000-00000000f1ee").expect("fixture UUIDv7")
}

fn reported(actor: Option<&str>, step: FleetStep) -> Telemetry {
    Telemetry::Fleet {
        actor: actor.map(str::to_owned),
        step,
    }
}

/// The keys and values one step carries, in order.
type Carried = [(&'static str, &'static str); 2];

/// Every step, each with the name the funnel matches on and the exact keys
/// and values it carries.
fn every_step() -> [(&'static str, FleetStep, Carried); 2] {
    [
        (
            "fleet_created",
            FleetStep::Created {
                fleet_id: fleet(),
                origin: FleetOrigin::Cli,
            },
            [
                ("fleet_id", "01924f4e-0000-7000-8000-00000000f1ee"),
                ("origin", "cli"),
            ],
        ),
        (
            "fleet_access_changed",
            FleetStep::AccessChanged {
                fleet_id: fleet(),
                kind: "secret",
                change: AccessChange::Denied,
            },
            [
                ("fleet_id", "01924f4e-0000-7000-8000-00000000f1ee"),
                ("kind", "secret"),
            ],
        ),
    ]
}

/// Each step reports under the name the architecture doc's funnel table
/// declares, attributed to the person who acted.
#[test]
fn should_report_each_fleet_step_under_its_published_name() {
    for (name, step, _) in every_step() {
        let telemetry = reported(Some(ACTOR), step);
        assert_eq!(telemetry.name(), name);
        assert_eq!(telemetry.event().event_name(), name);
        assert_eq!(telemetry.actor(), Some(ACTOR));
        assert_eq!(telemetry.event().distinct_id(), ACTOR);
    }
}

/// A grant the fleet asked for has no person behind it, so it is reported
/// anonymously rather than inside somebody's funnel.
#[test]
fn should_attribute_a_fleets_own_request_to_nobody() {
    let requested = reported(
        None,
        FleetStep::AccessChanged {
            fleet_id: fleet(),
            kind: "integration",
            change: AccessChange::Requested,
        },
    );

    assert_eq!(requested.actor(), None);
    assert_eq!(requested.name(), "fleet_access_changed");
}

/// The privacy allowlist: a step carries EXACTLY its keys, each an
/// identifier or a closed word — no message, name, secret or repository.
#[test]
fn should_carry_exactly_the_allowlisted_identifiers_and_words() {
    for (name, step, expected) in every_step() {
        let event = reported(Some(ACTOR), step.clone()).event();
        let carried: BTreeSet<&str> = event.properties().keys().map(String::as_str).collect();
        let allowed: BTreeSet<&str> = match step {
            FleetStep::Created { .. } => ["fleet_id", "origin"].into(),
            FleetStep::AccessChanged { .. } => ["fleet_id", "kind", "change"].into(),
        };
        assert_eq!(carried, allowed, "{name} carries only its allowlist");
        for (key, value) in expected {
            assert_eq!(
                event
                    .properties()
                    .get(key)
                    .and_then(|carried| carried.as_str()),
                Some(value),
                "{name}.{key}"
            );
        }
    }
}

/// Every change travels as its own word, so a funnel can split asks from
/// approvals.
#[test]
fn should_spell_every_change_and_origin_as_published() {
    let changes = [
        AccessChange::Requested,
        AccessChange::Approved,
        AccessChange::Denied,
        AccessChange::Revoked,
    ]
    .map(AccessChange::as_str);
    assert_eq!(changes, ["requested", "approved", "denied", "revoked"]);
    assert_eq!(
        [FleetOrigin::Cli, FleetOrigin::Dashboard].map(FleetOrigin::as_str),
        ["cli", "dashboard"]
    );
}
