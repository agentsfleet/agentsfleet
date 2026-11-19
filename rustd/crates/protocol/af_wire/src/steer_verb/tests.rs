//! The steer verb's body bounds, and the stored body a target reads.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use garde::Validate as _;

use super::{FLEET_ID_BYTES, FleetSteerAccepted, FleetSteerRequest};
use crate::event::{FleetSteerBody, STEER_MESSAGE_MAX_BYTES, message_of, sender_line};

const FLEET: &str = "0190d5c2-7e1a-7000-8000-00000000f001";
const SENDER: &str = "0190d5c2-7e1a-7000-8000-00000000f002";
const EVENT: &str = "0190d5c2-7e1a-7000-8000-00000000e001";

fn request(fleet_id: &str, text: String) -> FleetSteerRequest<'static> {
    FleetSteerRequest {
        fencing_token: 7,
        fleet_id: Cow::Owned(fleet_id.to_owned()),
        text: Cow::Owned(text),
    }
}

/// The fields a refused request names, in the order `garde` reports them.
fn refused_fields(request: &FleetSteerRequest<'_>) -> Vec<String> {
    request.validate().err().map_or_else(Vec::new, |report| {
        report.iter().map(|(path, _)| path.to_string()).collect()
    })
}

/// The bound half: a text at the cap passes; one byte more, an
/// empty text and a NUL are refused on `text`, a short id on `fleet_id`; an
/// unknown field is refused by the strict body before `garde` ever runs.
#[test]
fn fleet_steer_request_refuses_bad_bodies() {
    request(FLEET, "a".repeat(STEER_MESSAGE_MAX_BYTES))
        .validate()
        .expect("a text at the cap passes");
    for (text, why) in [
        ("a".repeat(STEER_MESSAGE_MAX_BYTES + 1), "8193 bytes"),
        (String::new(), "an empty text"),
        ("a\0b".to_owned(), "a NUL"),
    ] {
        assert_eq!(refused_fields(&request(FLEET, text)), ["text"], "{why}");
    }
    assert_eq!(
        refused_fields(&request("not-a-uuid", "hi".to_owned())),
        ["fleet_id"],
        "a fleet id of the wrong length"
    );
    let unknown = format!(
        r#"{{"fencing_token":7,"fleet_id":"{FLEET}","text":"hi","policy":{{"tools":["shell"]}}}}"#
    );
    let refused = serde_json::from_str::<FleetSteerRequest<'_>>(&unknown)
        .expect_err("a field the verb does not read is refused");
    assert!(
        refused.to_string().contains("unknown field `policy`"),
        "{refused}"
    );
    assert_eq!(FLEET.len(), FLEET_ID_BYTES);
}

/// The stored body puts the sender line first, so every reader of `message`
/// sees who asked and the id to answer with.
#[test]
fn a_fleet_steer_body_leads_with_the_sender_line() {
    let body = FleetSteerBody::new("run the hourly check", SENDER, "chief-of-staff", EVENT);
    let line = sender_line("chief-of-staff", SENDER);
    assert_eq!(body.message, format!("{line}\nrun the hourly check"));
    assert!(line.contains(SENDER) && line.contains("fleet_steer"));
    let stored = serde_json::to_string(&body).expect("the body serializes");
    assert_eq!(message_of(&stored), body.message);
    let read: FleetSteerBody<'_> = serde_json::from_str(&stored).expect("the body reads back");
    assert_eq!(read, body);
}

/// The reply a runner decodes tolerates a field a newer daemon adds.
#[test]
fn the_accepted_reply_tolerates_a_new_field() {
    let reply: FleetSteerAccepted<'_> =
        serde_json::from_str(r#"{"event_id":"e","replayed":true,"later":1}"#)
            .expect("a newer reply decodes");
    assert!(reply.replayed);
}
