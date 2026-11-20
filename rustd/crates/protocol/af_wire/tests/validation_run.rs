//! A fleet run's required fields: the history it carries, whether the held
//! sandbox resumes, whether input waits, and whose work it is.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a run that will not decode is an unmet precondition"
)]

use af_wire::run::RunPayload;

/// The run the daemon sends today.
const RUN: &str = r#"{"run_id":"l","fencing_token":1,"run_expires_at":"1970-01-01T00:00:00.000Z",
        "event":{"turn_id":"e","fleet_id":"f","workspace_id":"w","actor":"a",
        "event_type":"chat","request_json":"{}","created_at":"1970-01-01T00:00:00.000Z"},
        "policy":{"network_policy":{"allow":[],"read_only":true,"read_post_paths":[]},
        "tools":[],"secrets_map":null,"mintable":[],"provider":"p","api_key":"k",
        "inference_host":"h","base_url":null,"repository_binding":null,
        "http_origin_policies":[],"context":{"tool_window":1,"memory_checkpoint_every":1,
        "stage_chunk_threshold":0.5,"model":"m","context_cap_tokens":0}},
        "instructions":"","history":[],"resume_hold":false,"inputs_waiting":true,
        "owner":{"kind":"fleet","fleet_id":"f"}}"#;

/// The daemon always names the fleet's history, whether the held sandbox
/// resumes, whether input waits and whose work the run is; a run missing any
/// of them is refused rather than read as a default.
#[test]
fn a_run_must_name_its_history_resume_hold_waiting_inputs_and_owner() {
    let text = RUN;
    for key in [
        r#","history":[]"#,
        r#","resume_hold":false"#,
        r#","inputs_waiting":true"#,
        r#",
        "owner":{"kind":"fleet","fleet_id":"f"}"#,
    ] {
        let missing = text.replace(key, "");
        assert_ne!(missing, text, "{key} is in the fixture");
        assert!(
            serde_json::from_str::<RunPayload<'_>>(&missing).is_err(),
            "a run without {key} must be refused"
        );
    }
}

/// Earlier turns round-trip in order.
#[test]
fn test_run_history_round_trips() {
    let turns = serde_json::json!([{"message": "m1", "answer": "a1"}]);
    let carried = RUN.replace(r#""history":[]"#, &format!(r#""history":{turns}"#));
    let payload: RunPayload<'_> = serde_json::from_str(&carried).unwrap();
    assert_eq!(serde_json::to_value(&payload).unwrap()["history"], turns);
}

/// A run that resumes the held sandbox says so, and round-trips.
#[test]
fn test_run_resume_hold_round_trips() {
    let text = RUN;
    let fresh: RunPayload<'_> = serde_json::from_str(text).unwrap();
    let resumed = text.replace(r#""resume_hold":false"#, r#""resume_hold":true"#);
    let present: RunPayload<'_> = serde_json::from_str(&resumed).unwrap();

    assert!(!fresh.resume_hold);
    assert!(present.resume_hold);
    assert_eq!(serde_json::to_value(&present).unwrap()["resume_hold"], true);
}
