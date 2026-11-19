//! Ordinary recovery refuses ambiguous authority and never prints private root contents.

use af_wire::lease::{
    TURN_CHECKPOINT_BYTES, TurnCheckpointCommand, TurnCheckpointKind, TurnPrivateRoot,
};
use garde::Validate;
use serde_json::json;

const ID: &str = "01900000-0000-7000-8000-000000000001";

type Checked = Result<(), Box<dyn std::error::Error>>;

fn required<T>(value: Option<T>) -> serde_json::Result<T> {
    value
        .ok_or_else(|| <serde_json::Error as serde::de::Error>::custom("complete recovery fixture"))
}

fn paths(result: Result<(), garde::Report>) -> Vec<String> {
    result
        .err()
        .into_iter()
        .flat_map(|report| {
            report
                .iter()
                .map(|(path, _error)| path.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn begin() -> serde_json::Value {
    json!({"fencing_token":7,"operation_id":ID,"action":{
        "kind":"begin","expected_head":null,"format_version":1,
        "workspace":{"kind":"api_only"},"history_digest":"a".repeat(64),
        "page_count":0,"upload_budget_bytes":0,"logical_bytes":0,"entry_count":0
    }})
}

#[test]
fn native_turn_commands_use_reports_and_count_escaped_envelope_bytes() -> Checked {
    use af_wire::{
        lease::TurnMessageSize,
        protocol::{Down, StreamKind, Up},
    };
    let value = json!({"type":"turn_checkpoint","request_id":2,"lease_id":ID,"request":begin()});
    let encoded = serde_json::to_string(&value)?;
    let request: Up<'_> = serde_json::from_str(&encoded)?;
    request.validate()?;
    assert_eq!(request.stream_kind(), StreamKind::Reports);
    assert_eq!(request.request_id().map(std::num::NonZeroU64::get), Some(2));
    let response = json!({"type":"turn_checkpoint","request_id":2,"lease_id":ID,"operation_id":ID,"kind":"abandon",
        "answer":{"status":"ok","response":{"kind":"abandoned"}}});
    let encoded = serde_json::to_string(&response)?;
    let response: Down<'_> = serde_json::from_str(&encoded)?;
    assert_eq!(response.stream_kind(), StreamKind::Reports);
    assert_eq!(
        response.request_id().map(std::num::NonZeroU64::get),
        Some(2)
    );
    TurnMessageSize {
        bytes: TURN_CHECKPOINT_BYTES,
    }
    .validate()?;
    assert!(
        TurnMessageSize {
            bytes: TURN_CHECKPOINT_BYTES + 1
        }
        .validate()
        .is_err()
    );
    let root = TurnPrivateRoot {
        history: "\u{0001}".repeat(TURN_CHECKPOINT_BYTES / 4),
    };
    root.validate()?;
    let encoded = serde_json::to_vec(&root)?;
    assert!(
        TurnMessageSize {
            bytes: encoded.len()
        }
        .validate()
        .is_err()
    );
    Ok(())
}

#[test]
fn capture_authority_cannot_name_another_event_or_bucket() -> Checked {
    let encoded = serde_json::to_string(&begin())?;
    let parsed: TurnCheckpointCommand = serde_json::from_str(&encoded)?;
    parsed.validate()?;
    assert_eq!(parsed.action.kind(), TurnCheckpointKind::Begin);
    for extra in [
        "event_id",
        "fleet_id",
        "workspace_id",
        "bucket",
        "object_path",
    ] {
        let mut request = begin();
        required(request.as_object_mut())?.insert(extra.into(), json!(ID));
        assert!(
            serde_json::from_value::<TurnCheckpointCommand>(request).is_err_and(|fault| fault
                .classify()
                == serde_json::error::Category::Data
                && fault.to_string().contains(extra))
        );
        let mut request = begin();
        required(
            request
                .get_mut("action")
                .and_then(serde_json::Value::as_object_mut),
        )?
        .insert(extra.into(), json!(ID));
        assert!(
            serde_json::from_value::<TurnCheckpointCommand>(request).is_err_and(|fault| fault
                .classify()
                == serde_json::error::Category::Data
                && fault.to_string().contains(extra))
        );
    }
    Ok(())
}

#[test]
fn ordinary_state_is_independently_versioned_and_fenced() -> serde_json::Result<()> {
    for value in [json!(0), json!(u64::MAX)] {
        let mut request = begin();
        *required(request.get_mut("fencing_token"))? = value;
        let request: TurnCheckpointCommand = serde_json::from_value(request)?;
        assert_eq!(paths(request.validate()), vec!["fencing_token"]);
    }
    let mut request = begin();
    *required(
        request
            .get_mut("action")
            .and_then(|action| action.get_mut("format_version")),
    )? = json!(2);
    let request: TurnCheckpointCommand = serde_json::from_value(request)?;
    let invalid = paths(request.validate());
    assert_eq!(invalid.len(), 1);
    assert!(
        invalid
            .iter()
            .any(|field| field.ends_with("format_version"))
    );
    for bad in [
        "01900000-0000-4000-8000-000000000001",
        "01900000-0000-7000-8000-00000000000A",
    ] {
        let mut request = begin();
        *required(request.get_mut("operation_id"))? = json!(bad);
        let request: TurnCheckpointCommand = serde_json::from_value(request)?;
        let invalid = paths(request.validate());
        assert_eq!(invalid.len(), 1);
        assert!(
            invalid
                .iter()
                .any(|field| field.starts_with("operation_id"))
        );
    }
    Ok(())
}

#[test]
fn a_building_workspace_has_no_serializable_recovery_variant() -> serde_json::Result<()> {
    let mut request = begin();
    *required(
        request
            .get_mut("action")
            .and_then(|action| action.get_mut("workspace")),
    )? = json!({"kind":"building"});
    assert!(
        serde_json::from_value::<TurnCheckpointCommand>(request).is_err_and(|fault| fault
            .classify()
            == serde_json::error::Category::Data
            && fault.to_string().contains("building"))
    );
    Ok(())
}

#[test]
fn checkpoint_debug_output_does_not_quote_root_history() -> Checked {
    let private = "private-saved-conversation";
    let request: TurnCheckpointCommand = serde_json::from_value(json!({
        "fencing_token":7,"operation_id":ID,"action":{
            "kind":"commit","checkpoint":{"checkpoint_id":ID,"pin_id":ID},
            "root":{"history":private},"manifest_root":"a".repeat(64),"folds":[]
        }
    }))?;
    request.validate()?;
    assert!(!format!("{request:?}").contains(private));
    let too_large = TurnPrivateRoot {
        history: "x".repeat(TURN_CHECKPOINT_BYTES + 1),
    };
    assert_eq!(paths(too_large.validate()), vec!["history"]);
    Ok(())
}
