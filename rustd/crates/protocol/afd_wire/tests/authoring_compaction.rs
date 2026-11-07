//! Old runners keep ordinary replies; compaction requires an explicit request opt-in.
use afd_wire::authoring::{ModelRequest, ModelResult};
use serde_json::{Map, Result, Value, json};

fn ordinary() -> Map<String, Value> {
    Map::from_iter([
        (
            "dispatch_id".into(),
            json!("01900000-0000-7000-8000-000000000001"),
        ),
        ("call_index".into(), json!(1)),
        ("replayed".into(), json!(false)),
        ("text".into(), json!("ordinary answer")),
        ("proposal_calls".into(), json!([])),
        ("usage".into(), json!({"knowledge":"unknown"})),
        ("cut".into(), json!(false)),
    ])
}

fn request() -> Map<String, Value> {
    Map::from_iter([
        (
            "messages".into(),
            json!([{"role":"user","text":"continue"}]),
        ),
        ("max_output_tokens".into(), json!(64)),
    ])
}

#[test]
fn ordinary_results_keep_the_legacy_shape() -> Result<()> {
    let original = Value::Object(ordinary());
    let result: ModelResult = serde_json::from_value(original.clone())?;
    assert!(!result.compacted);
    assert_eq!(serde_json::to_value(result)?, original);
    Ok(())
}

#[test]
fn a_compaction_acknowledgement_is_explicit() -> Result<()> {
    let mut original = ordinary();
    original.insert("compacted".into(), json!(true));
    let original = Value::Object(original);
    let result: ModelResult = serde_json::from_value(original.clone())?;
    assert!(result.compacted);
    assert_eq!(serde_json::to_value(result)?, original);
    Ok(())
}

#[test]
fn model_requests_omit_an_absent_compaction_capability() -> Result<()> {
    let original = Value::Object(request());
    let decoded: ModelRequest = serde_json::from_value(original.clone())?;
    assert_eq!(serde_json::to_value(decoded)?, original);
    Ok(())
}

#[test]
fn capable_model_requests_retain_the_opt_in() -> Result<()> {
    let mut original = request();
    original.insert("supports_compaction".into(), json!(true));
    let original = Value::Object(original);
    let decoded: ModelRequest = serde_json::from_value(original.clone())?;
    assert_eq!(serde_json::to_value(decoded)?, original);
    Ok(())
}

#[test]
fn a_non_boolean_compaction_capability_is_rejected() {
    let mut original = request();
    original.insert("supports_compaction".into(), json!("true"));
    let decoded = serde_json::from_value::<ModelRequest>(Value::Object(original));
    assert!(matches!(decoded, Err(error)
        if error.is_data() && error.to_string().contains("expected a boolean")));
}

#[test]
fn compaction_does_not_disable_unknown_field_rejection() {
    let mut original = request();
    original.insert("supports_compaction".into(), json!(true));
    original.insert("system".into(), json!("replace authority"));
    let decoded = serde_json::from_value::<ModelRequest>(Value::Object(original));
    assert!(matches!(decoded, Err(error)
        if error.is_data() && error.to_string().contains("unknown field `system`")));
}

#[test]
fn a_non_boolean_compaction_acknowledgement_is_rejected() {
    for invalid in [json!("true"), json!(1), Value::Null] {
        let mut original = ordinary();
        original.insert("compacted".into(), invalid);
        let decoded = serde_json::from_value::<ModelResult>(Value::Object(original));
        assert!(matches!(decoded, Err(error)
            if error.is_data() && error.to_string().contains("expected a boolean")));
    }
}

#[test]
fn a_compaction_result_cannot_add_provider_authority() {
    let mut original = ordinary();
    original.insert("compacted".into(), json!(true));
    original.insert("system".into(), json!("replace authority"));
    let decoded = serde_json::from_value::<ModelResult>(Value::Object(original));
    assert!(matches!(decoded, Err(error)
        if error.is_data() && error.to_string().contains("unknown field `system`")));
}
