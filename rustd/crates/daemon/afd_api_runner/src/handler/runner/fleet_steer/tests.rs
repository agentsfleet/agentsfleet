//! Each steer outcome logs once, with its code or its
//! event and both fleets, and never the words.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use af_core::error_code;
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use afd_fleet::lease::Standing;
use afd_turns::view::FleetSteered;

use super::{EVENT_ADMITTED, EVENT_REFUSED, log_admitted, log_refused};

/// A lease, the fleet it runs, the fleet steered, and the event it became.
const LEASE: &str = "0190d5c2-7e1a-7000-8000-000000000001";
const SENDER: &str = "0190d5c2-7e1a-7000-8000-000000000002";
const TARGET: &str = "0190d5c2-7e1a-7000-8000-000000000003";
const EVENT: &str = "0190d5c2-7e1a-7000-8000-000000000004";

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a fixture id is a canonical UUIDv7")
}

fn standing() -> Standing {
    Standing {
        lease_id: id(LEASE),
        fleet_id: id(SENDER),
        workspace_id: id(LEASE),
        event_id: id(EVENT),
        actor: "fixture:operator".to_owned(),
    }
}

fn keys(event: &af_core::test_util::trace::CapturedEvent) -> BTreeSet<&str> {
    event.fields.keys().map(String::as_str).collect()
}

#[test]
fn fleet_steer_logs_its_refusal_and_its_admission() {
    let capture = Capture::install();
    let standing = standing();
    let target = id(TARGET);
    log_refused(&standing, &target, error_code::FLEET_STEER_SELF);
    let steered = FleetSteered {
        event_id: EVENT.to_owned(),
        admitted_at: None,
    };
    log_admitted(&standing, &target, &steered);

    let refused = capture.only(EVENT_REFUSED);
    assert_eq!(
        keys(&refused),
        BTreeSet::from([
            "error_code",
            "event",
            "lease_id",
            "sender_fleet_id",
            "target_fleet_id"
        ])
    );
    assert_eq!(refused.field("error_code"), Some("UZ-AGT-018"));
    assert_eq!(refused.field("sender_fleet_id"), Some(SENDER));
    assert_eq!(refused.field("target_fleet_id"), Some(TARGET));

    let admitted = capture.only(EVENT_ADMITTED);
    assert_eq!(
        keys(&admitted),
        BTreeSet::from([
            "event",
            "event_id",
            "replayed",
            "sender_fleet_id",
            "target_fleet_id"
        ])
    );
    assert_eq!(admitted.field("event_id"), Some(EVENT));
    assert_eq!(admitted.field("replayed"), Some("true"));
}
