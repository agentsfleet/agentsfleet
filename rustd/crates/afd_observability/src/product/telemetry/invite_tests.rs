//! The invite email's three events: one variant, three names, and a reply code
//! present only when the relay spoke one.

use serde_json::Value;

use super::{InviteEmailOutcome, Telemetry};

const OWNER: &str = "user_2owner";
const INVITE: &str = "0190f5a2-4b2d-7c11-8d5e-2a5f31d98210";

fn invite_email(outcome: InviteEmailOutcome) -> Telemetry {
    Telemetry::InviteEmail {
        actor: OWNER.to_owned(),
        tenant_id: "tenant".to_owned(),
        invite_id: INVITE.to_owned(),
        attempt: 2,
        outcome,
    }
}

/// Each outcome reports under the name the spec's metrics table declares,
/// attributed to the owner who sent it.
#[test]
fn should_name_each_invite_email_outcome() {
    let named = [
        ("invite_email_sent", InviteEmailOutcome::Sent { reply: 250 }),
        (
            "invite_email_failed",
            InviteEmailOutcome::Failed { reply: Some(550) },
        ),
        (
            "invite_email_unconfigured",
            InviteEmailOutcome::Unconfigured,
        ),
    ];
    for (name, outcome) in named {
        let telemetry = invite_email(outcome);
        assert_eq!(telemetry.name(), name);
        assert_eq!(telemetry.event().event_name(), name);
        assert_eq!(telemetry.actor(), Some(OWNER));
    }
}

/// The invite, the attempt and the reply travel as properties; a send with no
/// reply carries no `reply` key at all; no address is among them.
#[test]
fn should_carry_the_invite_and_reply_but_no_address() {
    let sent = invite_email(InviteEmailOutcome::Sent { reply: 250 }).event();
    let properties = sent.properties();
    assert_eq!(properties.get("invite_id"), Some(&Value::from(INVITE)));
    assert_eq!(properties.get("attempt"), Some(&Value::from(2)));
    assert_eq!(properties.get("reply"), Some(&Value::from(250)));
    assert!(
        properties
            .values()
            .all(|value| !value.to_string().contains('@'))
    );

    for outcome in [
        InviteEmailOutcome::Failed { reply: None },
        InviteEmailOutcome::Unconfigured,
    ] {
        let event = invite_email(outcome).event();
        assert!(!event.properties().contains_key("reply"));
    }
}
