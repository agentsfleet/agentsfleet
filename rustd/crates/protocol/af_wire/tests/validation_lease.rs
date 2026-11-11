//! A fleet lease's required fields: the history it carries, whether the held
//! sandbox resumes, and whether input waits.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a lease that will not decode is an unmet precondition"
)]

use af_wire::lease::LeasePayload;

/// The lease the daemon sends today.
const LEASE: &str = r#"{"lease_id":"l","fencing_token":1,"lease_expires_at":"1970-01-01T00:00:00.000Z",
        "event":{"event_id":"e","fleet_id":"f","workspace_id":"w","actor":"a",
        "event_type":"chat","request_json":"{}","created_at":"1970-01-01T00:00:00.000Z"},
        "policy":{"network_policy":{"allow":[],"read_only":true,"read_post_paths":[]},
        "tools":[],"secrets_map":null,"mintable":[],"provider":"p","api_key":"k",
        "inference_host":"h","base_url":null,"repository_binding":null,
        "http_origin_policies":[],"context":{"tool_window":1,"memory_checkpoint_every":1,
        "stage_chunk_threshold":0.5,"model":"m","context_cap_tokens":0}},
        "instructions":"","history":[],"resume_hold":false,"inputs_waiting":true}"#;

/// The daemon always names the fleet's history and whether the held sandbox
/// resumes; a lease missing either is refused rather than read as a default.
#[test]
fn a_lease_must_name_its_history_resume_hold_and_waiting_inputs() {
    let text = LEASE;
    for key in [
        r#","history":[]"#,
        r#","resume_hold":false"#,
        r#","inputs_waiting":true"#,
    ] {
        let missing = text.replace(key, "");
        assert_ne!(missing, text, "{key} is in the fixture");
        assert!(
            serde_json::from_str::<LeasePayload<'_>>(&missing).is_err(),
            "a lease without {key} must be refused"
        );
    }
}

/// Earlier turns round-trip in order.
#[test]
fn test_lease_history_round_trips() {
    let turns = serde_json::json!([{"message": "m1", "answer": "a1"}]);
    let carried = LEASE.replace(r#""history":[]"#, &format!(r#""history":{turns}"#));
    let payload: LeasePayload<'_> = serde_json::from_str(&carried).unwrap();
    assert_eq!(serde_json::to_value(&payload).unwrap()["history"], turns);
}

/// A lease that resumes the held sandbox says so, and round-trips.
#[test]
fn test_lease_resume_hold_round_trips() {
    let text = LEASE;
    let fresh: LeasePayload<'_> = serde_json::from_str(text).unwrap();
    let resumed = text.replace(r#""resume_hold":false"#, r#""resume_hold":true"#);
    let present: LeasePayload<'_> = serde_json::from_str(&resumed).unwrap();

    assert!(!fresh.resume_hold);
    assert!(present.resume_hold);
    assert_eq!(serde_json::to_value(&present).unwrap()["resume_hold"], true);
}
