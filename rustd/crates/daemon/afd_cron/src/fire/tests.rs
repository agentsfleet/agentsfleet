//! A fire's complete display payload preserves the normal runner message.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use af_core::id::Uuid7;
use af_wire::event::message_of;
use serde_json::json;

use super::{FireTarget, body};
use crate::DesiredStatus;

// Value: protects=display metadata cannot replace, decode or truncate customer intent;
// fails_when=the message becomes nested, parsed as JSON or loses Unicode/escapes;
// seam=normal event message parser; no_claim=storage, signatures or admission.
#[test]
fn schedule_payloads_preserve_complete_messages_and_resolved_display_facts() {
    let schedule = Uuid7::parse("0199f3c2-7a1b-7c3d-8e4f-0123456789ab").expect("schedule");
    let fleet = Uuid7::parse("0199f3c2-7a1b-7c3d-8e4f-0123456789ac").expect("fleet");
    let workspace = Uuid7::parse("0199f3c2-7a1b-7c3d-8e4f-0123456789ad").expect("workspace");
    for message in [
        "weekly check",
        "re-check \"error rate\"\nthen post\tthe number",
        r#"{"message":"nested"}"#,
        "Support triage — 東京 🦉",
    ] {
        let target = FireTarget {
            scheduled_at: None,
            fleet: fleet.clone(),
            workspace: workspace.clone(),
            message: message.to_owned(),
            description: "At 9:00 AM (Asia/Kolkata)".to_owned(),
            once: false,
            desired_status: DesiredStatus::Active,
            fleet_status: "active".to_owned(),
        };
        let stored = body(&schedule, &target).expect("typed fire payload");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored).expect("JSON"),
            json!({
                "message": message,
                "schedule": {"schedule_id": schedule.as_str(), "description": "At 9:00 AM (Asia/Kolkata)"}
            })
        );
        assert_eq!(message_of(&stored), message);
    }
}
