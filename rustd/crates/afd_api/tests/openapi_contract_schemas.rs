//! The schemas and media types in the generated document whose shape is the
//! promise: two Rust types sharing a name kept as two components, a tar named
//! as binary bytes, and a stream published as events.
//!
//! The document-wide gates — bodies described, references resolved, writes
//! naming their body — are `openapi_contract.rs`; these are the cases neither
//! gate would notice reverting.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

use crate::openapi_contract::{SCHEMA_PREFIX, document};

/// The lease's egress rules and the runner's posture are two schemas.
///
/// Both Rust types are named `NetworkPolicy`, and utoipa keys components by
/// name alone. Without the aliases the document would say a run's egress rules
/// were a three-word string, and every reference would still resolve.
#[test]
fn test_the_run_egress_rules_and_the_runner_posture_are_two_schemas() {
    let document = document();
    let schemas = document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .expect("the document carries schemas");
    let shape_of = |owner: &str| -> Option<serde_json::Value> {
        schemas
            .get(owner)?
            .get("properties")?
            .get("network_policy")?
            .get("$ref")?
            .as_str()?
            .strip_prefix(SCHEMA_PREFIX)
            .and_then(|name| schemas.get(name))
            .and_then(|schema| schema.get("type"))
            .cloned()
    };

    assert_eq!(
        shape_of("ExecutionPolicy"),
        Some(serde_json::json!("object")),
        "a run's egress rules are an allow list, not a posture word"
    );
    assert_eq!(
        shape_of("AssignedPolicy"),
        Some(serde_json::json!("string")),
        "a runner's posture is one of three words"
    );
}

/// The one route answering a tar: where a runner fetches a bundle.
const BUNDLE: &str = "/v1/runners/me/bundles/{content_hash}";

/// The media type a bundle is published under.
const TAR: &str = "application/x-tar";

/// The content of the `200` a `GET` at `path` answers, when one is described.
fn ok_content<'d>(document: &'d serde_json::Value, path: &str) -> Option<&'d serde_json::Value> {
    document
        .get("paths")?
        .get(path)?
        .get("get")?
        .get("responses")?
        .get("200")?
        .get("content")
}

/// The media types that `200` is published under.
fn media_types(document: &serde_json::Value, path: &str) -> Option<Vec<String>> {
    ok_content(document, path)?
        .as_object()
        .map(|content| content.keys().cloned().collect())
}

/// The component schema the bundle's tar body references.
fn tar_schema(document: &serde_json::Value) -> Option<&serde_json::Value> {
    let name = ok_content(document, BUNDLE)?
        .get(TAR)?
        .get("schema")?
        .get("$ref")?
        .as_str()?
        .strip_prefix(SCHEMA_PREFIX)?;
    document.get("components")?.get("schemas")?.get(name)
}

/// A tar is binary bytes under its own media type, and a stream is events.
///
/// utoipa reads a byte slice as an array of integers, which a generated client
/// parses as JSON and fails on the first byte of a tar; the document names a
/// binary string instead. Neither the body gate nor the reference gate would
/// notice that reverting, nor a stream published under `application/json`.
#[test]
fn test_the_tar_and_the_streams_publish_under_their_own_media_types() {
    let document = document();
    let tar = tar_schema(&document);

    assert_eq!(media_types(&document, BUNDLE), Some(vec![TAR.to_owned()]));
    assert_eq!(
        tar.and_then(|schema| schema.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("string")
    );
    assert_eq!(
        tar.and_then(|schema| schema.get("format"))
            .and_then(serde_json::Value::as_str),
        Some("binary"),
        "a byte array is parsed as JSON by every generated client"
    );
    for path in [
        "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream",
        "/v1/workspaces/{workspace_id}/events/stream",
    ] {
        assert_eq!(
            media_types(&document, path),
            Some(vec!["text/event-stream".to_owned()]),
            "{path}"
        );
    }
}
