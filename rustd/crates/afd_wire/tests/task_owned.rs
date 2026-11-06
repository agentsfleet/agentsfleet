//! Recovery responses survive wiping their transport buffers without losing authority.
use afd_wire::task::{TaskBindingsResponse, TaskContext, TaskInputsResponse};
use serde_json::{Value, json};

fn context() -> Value {
    json!({
        "task_id":"task", "originating_item_id":"item", "target":{"kind":"api_only"},
        "generation":7, "stop_generation":2, "accepted_revision_id":"revision",
        "objective":"Retain the accepted goal", "goal_digest":"digest", "questions":[],
        "state":"running", "covered_input_sequence":3, "input_sequence":5,
        "effect_cursor":9, "confirmed_effect_watermark":8, "checkpoint_id":"checkpoint",
        "checkpoint_generation":6, "expires_at":"2026-11-01T00:00:00Z",
        "limits":{"model_nanos":"1000","wall_seconds":40,"running_seconds":30,
            "leases":10,"publications":4,"decision_calls":20,"fallback_reads":6},
        "usage":{"model_nanos":"100","reserved_model_nanos":"20","running_nanos":300,
            "leases":2,"publications":1,"decision_calls":3,"fallback_reads":4}
    })
}

#[test]
fn owned_context_preserves_every_recovery_cursor_after_source_is_dropped() -> serde_json::Result<()>
{
    for checkpoint in [json!("checkpoint"), Value::Null] {
        let mut expected = context();
        if let Value::Object(fields) = &mut expected {
            fields.insert("checkpoint_id".into(), checkpoint);
        }
        let encoded = serde_json::to_string(&expected)?;
        let decoded: TaskContext<'_> = serde_json::from_str(&encoded)?;
        let owned: TaskContext<'static> = decoded.into_owned();
        drop(encoded);
        assert_eq!(serde_json::to_value(owned)?, expected);
    }
    Ok(())
}

#[test]
fn owned_bindings_preserve_deadline_and_association_after_transport_drop() -> serde_json::Result<()>
{
    for current in [context(), Value::Null] {
        let expected = json!({"items":[{"item_id":"item","task_id":"task",
            "task_generation":7,"action":"steer","applies_here":true,"input_sequence":5}],
            "current_task":current,"execution_deadline":"2026-10-31T00:00:00Z","replayed":true});
        let encoded = serde_json::to_string(&expected)?;
        let decoded: TaskBindingsResponse<'_> = serde_json::from_str(&encoded)?;
        let owned: TaskBindingsResponse<'static> = decoded.into_owned();
        drop(encoded);
        assert_eq!(serde_json::to_value(owned)?, expected);
    }
    Ok(())
}

fn inputs(current: &Value) -> Value {
    json!({"source_admission_id":"source","accepted_revision_id":"revision",
        "current_message":"original message", "inputs":[{"source_admission_id":"next",
            "sequence":4,"message":"next message","accepted":null}],
        "accepted_inputs":[{"source_admission_id":"source","plan_digest":"plan",
            "item_id":"item","task":{"task_id":"task","generation":7},"sequence":5,
            "action":"steer","original_message":"original message","accepted_text":"message",
            "interpretation":"accepted meaning","applied":true,"response_text":"confirmed reply"}],
        "current_task":current,"execution_deadline":"2026-10-31T00:00:00Z",
        "current_interpretation":{"source_admission_id":"source","plan_digest":"plan",
            "message":"original message","items":[]},
        "tasks":[{"task_id":"task","title":"Repair","state":"running","generation":7}],
        "next_cursor":"opaque-next"})
}

#[test]
fn owned_inputs_preserve_accepted_meaning_and_reply_without_reclassification()
-> serde_json::Result<()> {
    for current in [context(), Value::Null] {
        let expected = inputs(&current);
        let encoded = serde_json::to_string(&expected)?;
        let decoded: TaskInputsResponse<'_> = serde_json::from_str(&encoded)?;
        let owned: TaskInputsResponse<'static> = decoded.into_owned();
        drop(encoded);
        assert_eq!(serde_json::to_value(owned)?, expected);
    }
    Ok(())
}
