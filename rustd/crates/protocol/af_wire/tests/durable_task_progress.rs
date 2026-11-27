//! Public progress and private transfer descriptors retain distinct exposure rules.
use af_wire::{
    activity::{ActivityFrame, DurableTaskActivityRecord},
    durable_task::{
        CheckpointCipher, CheckpointObject, CheckpointTransfer, DurableTaskEffectState,
        DurableTaskInputAction,
    },
};
use serde_json::json;

#[test]
fn retained_durable_task_progress_preserves_live_facts_without_tool_call_identity()
-> serde_json::Result<()> {
    let progress = json!({"activity_id":"01900000-0000-7000-8000-000000000001",
        "durable_task_id":null,"source_turn_id":null,"operation":"capture","phase":"uploading",
        "provider":null,"model":null,"completed":2,"total":3,
        "bytes":{"acknowledged":20,"missing":10,"reused":30},"sequence":7});
    for kind in [
        "durable_task_decision_started",
        "durable_task_decision_completed",
        "durable_task_transfer_progress",
    ] {
        let encoded = serde_json::to_string(&json!({kind:progress}))?;
        let frame: ActivityFrame<'_> = serde_json::from_str(&encoded)?;
        assert_eq!(frame.call_id(), None);
        let record = frame.durable_task_activity().ok_or_else(|| {
            <serde_json::Error as serde::de::Error>::custom(
                "task frame must retain public progress",
            )
        })?;
        assert_eq!(serde_json::to_value(record.progress())?, progress);
        let saved = json!({"kind":kind,"progress":progress});
        assert_eq!(serde_json::to_value(&record)?, saved);
        assert_eq!(
            serde_json::from_value::<DurableTaskActivityRecord>(saved)?,
            record
        );
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
        "status":"missing","cipher":"aes_256_gcm","nonce":"secret-nonce","data_key":"secret-key",
        "ciphertext_sha256":"private-ciphertext-digest","ciphertext_length":26,"transfer":transfer});
    let object: CheckpointObject = serde_json::from_value(expected.clone())?;
    assert_eq!(
        format!("{object:?}"),
        "CheckpointObject { object_id: \"public-object\", status: Missing, .. }"
    );
    assert_eq!(serde_json::to_value(object)?, expected);
    Ok(())
}

// The cipher's spelling is what the daemon's file, each checkpoint row and each
// authorization frame carry, so one enum writes and reads all three.
#[test]
fn a_checkpoint_cipher_has_one_spelling_for_the_file_the_row_and_the_frame()
-> Result<(), Box<dyn std::error::Error>> {
    for (cipher, spelling) in [
        (CheckpointCipher::Aes256Gcm, "aes_256_gcm"),
        (CheckpointCipher::Plain, "plain"),
    ] {
        assert_eq!(<&str>::from(cipher), spelling);
        assert_eq!(spelling.parse::<CheckpointCipher>().ok(), Some(cipher));
        assert_eq!(serde_json::to_value(cipher)?, json!(spelling));
        assert_eq!(
            serde_json::from_value::<CheckpointCipher>(json!(spelling))?,
            cipher
        );
    }
    assert_eq!(CheckpointCipher::default(), CheckpointCipher::Aes256Gcm);
    for unknown in ["aes256_gcm", "AES_256_GCM", "none", ""] {
        assert_eq!(unknown.parse::<CheckpointCipher>().ok(), None, "{unknown}");
        let decoded = serde_json::from_value::<CheckpointCipher>(json!(unknown)).ok();
        assert_eq!(decoded, None, "{unknown}");
    }
    Ok(())
}

#[test]
fn stored_action_and_effect_spellings_match_public_json() -> serde_json::Result<()> {
    for spelling in ["steer", "new_work"] {
        let action: DurableTaskInputAction = serde_json::from_value(json!(spelling))?;
        assert_eq!(action.as_str(), spelling);
        assert_eq!(serde_json::to_value(action)?, json!(spelling));
    }
    // The intake's routing answers are gone; only a tool's two actions bind.
    for retired in ["conversation", "status", "refresh", "clarify"] {
        let refused = serde_json::from_value::<DurableTaskInputAction>(json!(retired)).err();
        assert!(refused.is_some(), "{retired} must not decode");
    }
    for spelling in ["prepared", "issued", "confirmed", "unknown", "refused"] {
        let state: DurableTaskEffectState = serde_json::from_value(json!(spelling))?;
        assert_eq!(state.as_str(), spelling);
        assert_eq!(serde_json::to_value(state)?, json!(spelling));
    }
    Ok(())
}
