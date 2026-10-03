//! The memory verbs' response shapes, pinned here because nothing else would
//! notice a renamed or reordered field — and a response body no test pins is
//! exactly the shape a handler used to spell inline with `json!`.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a shape that will not serialize is an unmet precondition"
)]

use std::borrow::Cow;

use afd_wire::memory::{
    MemoryCaptureResponse, MemoryDelta, MemoryHydrateResponse, PINNED_CATEGORY, RECALL_LIMIT_MAX,
    SharedMemory, Visibility,
};
use garde::Validate as _;

/// A capture reply carries the two tallies a runner acts on, and only those.
///
/// Field NAMES, not just presence: a runner reads `stored` to know its memory
/// landed and `skipped` to know some was refused for shape. Renaming either
/// silently stops a runner reacting to a refusal it can fix.
#[test]
fn test_a_capture_reply_carries_stored_and_skipped() {
    let reply = MemoryCaptureResponse {
        stored: 3,
        skipped: 1,
    };

    let json = serde_json::to_value(&reply).unwrap();

    assert_eq!(json, serde_json::json!({"stored": 3, "skipped": 1}));
}

/// Both tallies survive a round trip through JSON.
#[test]
fn test_a_capture_reply_round_trips() {
    let reply = MemoryCaptureResponse {
        stored: 0,
        skipped: 12,
    };

    let bytes = serde_json::to_vec(&reply).unwrap();
    let back: MemoryCaptureResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(back, reply);
}

/// A fleet with no grant hydrates in the shape every runner already parses.
///
/// The Zig runner that ships reads this reply strictly, so a field it does not
/// know would leave every lease with empty memory. `shared` and `publish`
/// appear only once a grant makes them mean something.
#[test]
fn test_a_hydrate_reply_without_grants_keeps_the_shape_runners_parse() {
    let reply = MemoryHydrateResponse {
        memory: Vec::new(),
        shared: Vec::new(),
        publish: false,
    };

    let json = serde_json::to_value(&reply).unwrap();

    assert_eq!(json, serde_json::json!({"memory": []}));
}

/// A granted fleet's reply carries what it reads and that it may publish,
/// and a reply without them reads back as no grant.
#[test]
fn test_a_hydrate_reply_carries_grants_only_when_given() {
    let shared = SharedMemory {
        key: Cow::Borrowed("incident:41"),
        content: Cow::Borrowed("escalated"),
        category: Cow::Borrowed("core"),
        writer_fleet_id: Cow::Borrowed("fleet-2"),
        writer_fleet_name: Cow::Borrowed("triage"),
        updated_at: 7,
    };
    let reply = MemoryHydrateResponse {
        memory: Vec::new(),
        shared: vec![shared],
        publish: true,
    };

    let json = serde_json::to_value(&reply).unwrap();
    assert_eq!(
        json.pointer("/publish"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(
        json.pointer("/shared/0/writer_fleet_name")
            .and_then(serde_json::Value::as_str),
        Some("triage")
    );

    let bare: MemoryHydrateResponse<'_> = serde_json::from_str(r#"{"memory": []}"#).unwrap();
    assert!(!bare.publish);
    assert!(bare.shared.is_empty());
}

/// The published recall `limit` is the range the request type proves, so a
/// client generated from the spec never sends a limit the daemon refuses.
#[test]
fn test_the_published_recall_limit_is_the_proved_range() {
    let openapi = include_str!("../../../../public/openapi.json");
    let document: serde_json::Value = serde_json::from_str(openapi).unwrap();
    let limit = |bound: &str| {
        document
            .pointer(&format!(
                "/components/schemas/MemoryRecallRequest/properties/limit/{bound}"
            ))
            .and_then(serde_json::Value::as_u64)
    };
    assert_eq!(limit("minimum"), Some(1));
    assert_eq!(
        limit("maximum").and_then(|max| usize::try_from(max).ok()),
        Some(RECALL_LIMIT_MAX)
    );
}

/// A delta holding NUL in any text field is malformed, and the report names
/// that field: Postgres cannot store NUL in `text`, so one such delta would
/// fail the whole push's statement instead of being skipped as malformed.
#[test]
fn test_a_delta_holding_nul_is_malformed_on_every_text_field() {
    let clean = MemoryDelta {
        key: Cow::Borrowed("deploy_target"),
        content: Cow::Borrowed("fly in iad"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Fleet,
    };
    clean.validate().unwrap();

    let nul = Cow::Borrowed("before\0after");
    for (field, delta) in [
        (
            "key",
            MemoryDelta {
                key: nul.clone(),
                ..clean.clone()
            },
        ),
        (
            "content",
            MemoryDelta {
                content: nul.clone(),
                ..clean.clone()
            },
        ),
        (
            "category",
            MemoryDelta {
                category: nul.clone(),
                ..clean.clone()
            },
        ),
    ] {
        let report = delta.validate().unwrap_err();
        let paths: Vec<String> = report.iter().map(|(path, _)| path.to_string()).collect();
        assert_eq!(paths, [field], "{field}");
    }
}
