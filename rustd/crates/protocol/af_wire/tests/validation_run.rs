//! A fleet run's required fields: the fleet's name, the history it carries,
//! whether the held sandbox resumes, whether input waits, and whose work it is.
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
        "policy":{"network_policy":{"allow":[]},
        "tools":[],"secrets_map":null,"mintable":[],"provider":"p","api_key":"k",
        "inference_host":"h","base_url":null,"repository_binding":null,
        "http_origin_policies":[],"context":{"tool_window":1,"memory_checkpoint_every":1,
        "stage_chunk_threshold":0.5,"model":"m","context_cap_tokens":0}},
        "name":"support-triage","history":[],"resume_hold":false,"inputs_waiting":true,
        "owner":{"kind":"fleet","fleet_id":"f"}}"#;

/// The daemon always names the fleet, its history, whether the held sandbox
/// resumes, whether input waits and whose work the run is; a run missing any
/// of them is refused rather than read as a default.
#[test]
fn a_run_must_name_its_fleet_history_resume_hold_waiting_inputs_and_owner() {
    let text = RUN;
    for key in [
        r#""name":"support-triage","#,
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

/// Earlier turns round-trip in order, each with its sequence and, when its
/// run made any, its calls.
#[test]
fn test_run_history_round_trips() {
    let turns = serde_json::json!([
        {"sequence": 1, "actor": "steer:fixture", "event_type": "chat", "message": "m1",
            "images": [{"media_type": "image/png", "data": "iVBORw=="}], "answer": "a1"},
        {"sequence": 2, "actor": "machine", "event_type": "chat", "message": "m2", "calls": [{"call_id": "4:1", "tool": "shell",
            "arguments": {"command": "ls"}, "output": "a.txt", "status": "succeeded"}],
            "answer": "a2"},
    ]);
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

/// A turn that does not name its sequence is refused: a saved summary's
/// watermark could not place it.
#[test]
fn a_turn_without_its_sequence_is_refused() {
    let turns = serde_json::json!([{"message": "m1", "answer": "a1"}]);
    let carried = RUN.replace(r#""history":[]"#, &format!(r#""history":{turns}"#));
    assert!(
        serde_json::from_str::<RunPayload<'_>>(&carried).is_err(),
        "a turn without its sequence must be refused"
    );
}

/// A run names a fleet as its owner and nothing else: a draft owner, or any
/// other kind, fails to decode rather than running as a fleet's.
#[test]
fn a_run_payload_names_no_draft_owner() {
    let fleet = r#""owner":{"kind":"fleet","fleet_id":"f"}"#;
    for owner in [
        r#""owner":{"kind":"draft","draft_id":"d"}"#,
        r#""owner":{"kind":"draft"}"#,
        r#""owner":{"kind":"setup","fleet_id":"f"}"#,
    ] {
        let carried = RUN.replace(fleet, owner);
        assert_ne!(carried, RUN, "the fixture names a fleet owner");
        assert!(
            serde_json::from_str::<RunPayload<'_>>(&carried).is_err(),
            "{owner} must be refused"
        );
    }
}

/// The fleet's unfinished tasks ride the run so its model can name one; a run
/// that carries none decodes them as empty.
#[test]
fn a_runs_unfinished_tasks_round_trip_and_default_to_none() {
    let fresh: RunPayload<'_> = serde_json::from_str(RUN).unwrap();
    assert_eq!(fresh.unfinished_tasks, Vec::new());
    assert_eq!(fresh.name, "support-triage");

    let tasks = serde_json::json!([{"durable_task_id": "019a0000-0000-7000-8000-000000000002",
        "title": "Fix the null-cart error", "state": "waiting", "generation": 3}]);
    let carried = RUN.replace(
        r#""history":[]"#,
        &format!(r#""history":[],"unfinished_tasks":{tasks}"#),
    );
    let payload: RunPayload<'_> = serde_json::from_str(&carried).unwrap();
    assert_eq!(payload.unfinished_tasks.len(), 1);
    assert_eq!(
        serde_json::to_value(&payload).unwrap()["unfinished_tasks"],
        tasks
    );
}
