//! Public progress and private transfer descriptors retain distinct exposure rules.
use afd_wire::{
    activity::{ActivityFrame, TaskActivityRecord},
    task::{CheckpointObject, CheckpointTransfer, TaskEffectState, TaskInputAction},
};
use serde_json::json;

#[test]
fn retained_task_progress_preserves_live_facts_without_tool_call_identity() -> serde_json::Result<()>
{
    let progress = json!({"activity_id":"01900000-0000-7000-8000-000000000001",
        "task_id":null,"source_admission_id":null,"operation":"capture","phase":"uploading",
        "provider":null,"model":null,"completed":2,"total":3,
        "bytes":{"acknowledged":20,"missing":10,"reused":30},"sequence":7});
    for kind in [
        "task_decision_started",
        "task_decision_completed",
        "task_transfer_progress",
    ] {
        let encoded = serde_json::to_string(&json!({kind:progress}))?;
        let frame: ActivityFrame<'_> = serde_json::from_str(&encoded)?;
        assert_eq!(frame.call_id(), None);
        let record = frame.task_activity().ok_or_else(|| {
            <serde_json::Error as serde::de::Error>::custom(
                "task frame must retain public progress",
            )
        })?;
        assert_eq!(serde_json::to_value(record.progress())?, progress);
        let saved = json!({"kind":kind,"progress":progress});
        assert_eq!(serde_json::to_value(&record)?, saved);
        assert_eq!(serde_json::from_value::<TaskActivityRecord>(saved)?, record);
    }
    Ok(())
}

#[test]
fn checkpoint_debug_omits_keys_and_signed_transport_while_wire_retains_them()
-> serde_json::Result<()> {
    let transfer = json!({"url":"https://storage.test/private?signature=secret-signature",
        "headers":{"authorization":"secret-header"},"expires_at":"2026-11-01T00:00:00Z"});
    let decoded: CheckpointTransfer = serde_json::from_value(transfer.clone())?;
    assert_eq!(format!("{decoded:?}"), "CheckpointTransfer(redacted)");
    assert_eq!(serde_json::to_value(decoded)?, transfer);
    let expected = json!({"object_id":"public-object","repository":"private-repository",
        "plaintext":{"sha256":"private-plaintext-digest","size":10,"kind":"file"},
        "status":"missing","nonce":"secret-nonce","data_key":"secret-key",
        "ciphertext_sha256":"private-ciphertext-digest","ciphertext_length":26,"transfer":transfer});
    let object: CheckpointObject = serde_json::from_value(expected.clone())?;
    assert_eq!(
        format!("{object:?}"),
        "CheckpointObject { object_id: \"public-object\", status: Missing, .. }"
    );
    assert_eq!(serde_json::to_value(object)?, expected);
    Ok(())
}

#[test]
fn stored_action_and_effect_spellings_match_public_json() -> serde_json::Result<()> {
    for spelling in [
        "conversation",
        "status",
        "refresh",
        "steer",
        "new_work",
        "clarify",
    ] {
        let action: TaskInputAction = serde_json::from_value(json!(spelling))?;
        assert_eq!(action.as_str(), spelling);
        assert_eq!(serde_json::to_value(action)?, json!(spelling));
    }
    for spelling in ["prepared", "issued", "confirmed", "unknown", "refused"] {
        let state: TaskEffectState = serde_json::from_value(json!(spelling))?;
        assert_eq!(state.as_str(), spelling);
        assert_eq!(serde_json::to_value(state)?, json!(spelling));
    }
    Ok(())
}
