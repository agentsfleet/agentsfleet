//! The memory verbs' response shapes, pinned here because nothing else would
//! notice a renamed or reordered field — and a response body no test pins is
//! exactly the shape a handler used to spell inline with `json!`.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a shape that will not serialize is an unmet precondition"
)]

use std::borrow::Cow;

use afd_wire::memory::{
    MemoryCaptureResponse, MemoryDelta, MemoryHydrateResponse, MemoryRecallRequest,
    PINNED_CATEGORY, RECALL_LIMIT_MAX, SharedMemory, Visibility,
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
/// `agentsfleet-runner` reads this reply strictly, so a field it does not know
/// would leave every lease with empty memory. `shared` and `publish` appear
/// only once a grant makes them mean something.
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
    assert_eq!(bare.shared, [] as [afd_wire::memory::SharedMemory<'_>; 0]);
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

/// A recall query holding NUL is malformed, named on `query`: the search runs
/// as a Postgres parameter, where NUL fails the statement instead of matching
/// nothing, and a runner should hear "malformed", not "datastore error".
#[test]
fn test_a_recall_query_holding_nul_is_malformed() {
    let clean = MemoryRecallRequest {
        lease_id: Cow::Borrowed("lease-1"),
        fencing_token: 1,
        query: Cow::Borrowed("deploy"),
        limit: 1,
    };
    clean.validate().unwrap();

    let report = MemoryRecallRequest {
        query: Cow::Borrowed("dep\0loy"),
        ..clean
    }
    .validate()
    .unwrap_err();

    let paths: Vec<String> = report.iter().map(|(path, _)| path.to_string()).collect();
    assert_eq!(paths, ["query"]);
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

/// A shared entry read out of a reply outlives the reply's bytes with every
/// field in its own place: the text it borrowed is dropped before the entry
/// is read, which the borrow checker allows only for an entry that copied it.
#[test]
fn test_a_shared_entry_detached_from_its_reply_keeps_every_field() {
    let entry = SharedMemory {
        key: Cow::Borrowed("incident:41"),
        content: Cow::Borrowed("escalated"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        writer_fleet_id: Cow::Borrowed("fleet-2"),
        writer_fleet_name: Cow::Borrowed("triage"),
        updated_at: 7,
    };
    let text = serde_json::to_string(&entry).unwrap();
    let read: SharedMemory<'_> = serde_json::from_str(&text).unwrap();

    let detached = read.into_owned();
    drop(text);

    assert_eq!(detached, entry);
}
