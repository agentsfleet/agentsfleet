//! Which assignments are too large for one frame, and the failure report one
//! of them earns.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_validate::TEXT_MAX_CHARS;
use af_wire::protocol::MESSAGE_BYTES_MAX;
use af_wire::report::{FailureClass, Outcome};
use serde_json::{Value, json};

use super::{Message, Unframed, detail};

/// A fleet run's assignment, as the daemon frames one.
const RUN: &str = r#"{"type":"run","run":{"kind":"fleet","run_id":"0190d5c2-7e1a-7000-8000-00000000e001","fencing_token":4,"run_expires_at":"1970-01-01T00:00:00.931Z","event":{"turn_id":"0190d5c2-7e1a-7000-8000-00000000e002","fleet_id":"fleet_id","workspace_id":"workspace_id","actor":"steer:user_1","event_type":"chat","request_json":"{\"message\":\"hi\"}","created_at":"1970-01-01T00:00:00.426Z"},"policy":{"network_policy":{"allow":["allow"],"read_only":true,"read_post_paths":["read_post_paths"]},"tools":["tools"],"secrets_map":{"secrets_map":"secrets_map"},"mintable":[{"name":"name","integration":"integration"}],"provider":"provider","api_key":"api_key","inference_host":"inference_host","base_url":"base_url","repository_binding":{"repositories":["repositories"],"access":"read","base_branch":"base_branch"},"http_origin_policies":[{"host":"host","credential_names":["credential_names"],"requests":[{"method":"get","path":"path","path_match":"exact","json_fields":[{"name":"name","string_value":"string_value","boolean_value":true}]}]}],"context":{"tool_window":141,"memory_checkpoint_every":26,"stage_chunk_threshold":0.75,"model":"model","context_cap_tokens":499}},"instructions":"instructions","history":[],"resume_hold":false,"inputs_waiting":false}}"#;

/// [`RUN`] with its instructions replaced by `instructions`.
fn run_with(instructions: &str) -> String {
    let mut document: Value = serde_json::from_str(RUN).expect("the fixture is JSON");
    *document
        .pointer_mut("/run/instructions")
        .expect("the fixture carries instructions") = json!(instructions);
    serde_json::to_string(&document).expect("the fixture serializes")
}

/// An assignment that fits one frame is sent as it always was.
#[test]
fn a_run_that_fits_its_frame_is_not_failed() {
    let framed = run_with("Triage the support queue.");
    let message: Message<'_> = serde_json::from_str(&framed).expect("a run frame");
    assert_eq!(Unframed::of(&message), None);
}

/// [`RUN`] carrying the most text a run can: its instructions, a setup
/// candidate's instructions and the message, each at the ceiling in
/// `character`.
fn ceiling_run(character: char) -> String {
    let ceiling: String = std::iter::repeat_n(character, TEXT_MAX_CHARS).collect();
    let mut document: Value =
        serde_json::from_str(&run_with(&ceiling)).expect("the fixture is JSON");
    *document
        .pointer_mut("/run/event/request_json")
        .expect("the fixture carries a message") = json!(json!({ "message": ceiling }).to_string());
    document
        .pointer_mut("/run")
        .and_then(Value::as_object_mut)
        .expect("the fixture carries a run")
        .insert(
            "setup".to_owned(),
            json!({"sequence": 2, "save_requested": false, "tool": {},
                "candidate": {"id": "0190d5c2-7e1a-7000-8000-00000000e003", "revision": 1,
                    "definition": {"instructions_md": ceiling}}}),
        );
    serde_json::to_string(&document).expect("the fixture serializes")
}

/// The most text the routes accept still fits one frame, so the run is sent.
/// Three-byte characters fill each text's UTF-8. A control character fills
/// its JSON escape, six bytes, and seven in the message, whose own JSON is
/// quoted again inside the frame: the worst case, about 19 MiB.
#[test]
fn a_run_carrying_ceiling_instructions_and_a_ceiling_message_is_framed() {
    for (character, every_text) in [('界', 9_437_184), ('\u{1}', 19_922_944)] {
        let framed = ceiling_run(character);
        let message: Message<'_> = serde_json::from_str(&framed).expect("a run frame");
        assert_eq!(Unframed::of(&message), None, "{character:?}");
        let bytes = af_validate::serialized_len(&message).expect("the frame measures");
        assert!(
            bytes > every_text,
            "{character:?}: every text rides the frame: {bytes}"
        );
        assert!(bytes <= MESSAGE_BYTES_MAX, "{character:?}: {bytes}");
    }
}

/// Instructions that alone fill the frame push the assignment past it: the
/// run is named for failing, under the fence the assignment raised.
#[test]
fn a_run_past_its_frame_is_named_for_failing() {
    let unframed = run_with(&"x".repeat(MESSAGE_BYTES_MAX));
    let message: Message<'_> = serde_json::from_str(&unframed).expect("a run frame");
    assert_eq!(
        Unframed::of(&message),
        Some(Unframed {
            run_id: "0190d5c2-7e1a-7000-8000-00000000e001".to_owned(),
            turn_id: "0190d5c2-7e1a-7000-8000-00000000e002".to_owned(),
            fencing_token: 4,
            draft: false,
        })
    );
}

/// The failure report fails the run under the new class and names the code;
/// a draft's report carries an empty setup part, which routes it to the
/// draft's settlement, and a fleet's carries none.
#[test]
fn the_failure_report_names_its_class_and_code() {
    let fleet = Unframed {
        run_id: "run".to_owned(),
        turn_id: "turn".to_owned(),
        fencing_token: 9,
        draft: false,
    };
    let said = detail();
    let report = fleet.report(&said);
    assert_eq!(report.outcome, Outcome::FleetError);
    assert_eq!(report.failure_reason, Some(FailureClass::FrameTooLarge));
    assert!(report.failure_detail.starts_with("[UZ-RUN-032] "), "{said}");
    assert_eq!((report.fencing_token, report.tokens), (9, 0));
    assert_eq!(report.checkpoint.last_turn_id, "turn");
    assert_eq!(report.requests.0.len(), 0);
    assert_eq!(report.setup, None);

    let draft = Unframed {
        draft: true,
        ..fleet
    };
    let report = draft.report(&said);
    assert_eq!(report.setup.map(|setup| setup.proposal), Some(None));
}
