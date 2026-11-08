//! The schemas and media types in the generated document whose shape is the
//! promise: two Rust types sharing a name kept as two components, a tar named
//! as binary bytes, a stream published as events, and the two workspace routes
//! the dashboard's generated client is built from.
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

/// A stream is published as events, never as JSON.
///
/// A stream published under `application/json` would be parsed whole by every
/// generated client, which waits for a body that never ends.
#[test]
fn test_the_streams_publish_under_their_own_media_type() {
    let document = document();
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

/// Where a person creates a workspace.
const CREATE_WORKSPACE: &str = "/v1/workspaces";

/// Where a person lists the workspaces they can reach.
const LIST_WORKSPACES: &str = "/v1/tenants/me/workspaces";

/// The header a retried write would carry, which workspace creation does not read.
const IDEMPOTENCY_KEY: &str = "Idempotency-Key";

/// The operation at `method` `path`, as the generated document publishes it.
fn operation(document: &serde_json::Value, path: &str, method: &str) -> serde_json::Value {
    document
        .get("paths")
        .and_then(|paths| paths.get(path))
        .and_then(|item| item.get(method))
        .cloned()
        .expect("the route is published")
}

/// The names of an operation's parameters, in published order.
fn parameter_names(operation: &serde_json::Value) -> Vec<String> {
    operation
        .get("parameters")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|parameter| parameter.get("name")?.as_str().map(str::to_owned))
        .collect()
}

/// Creating a workspace is published as the dashboard calls it: by its operation
/// id, with an optional body, no idempotency header, and the refusals a client
/// must handle.
#[test]
fn test_workspace_creation_publishes_what_its_client_relies_on() {
    let document = document();
    let create = operation(&document, CREATE_WORKSPACE, "post");

    assert_eq!(
        create
            .get("operationId")
            .and_then(serde_json::Value::as_str),
        Some("create_workspace"),
        "{create}"
    );
    assert_ne!(
        create
            .get("requestBody")
            .and_then(|body| body.get("required"))
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "a workspace can be created with no body: {create}"
    );
    assert!(
        !parameter_names(&create)
            .iter()
            .any(|name| name.eq_ignore_ascii_case(IDEMPOTENCY_KEY)),
        "creation reads no idempotency header: {create}"
    );
    let responses = create.get("responses").expect("the operation answers");
    for status in ["401", "403", "409", "500"] {
        assert!(
            responses.get(status).is_some(),
            "{status} is published: {responses}"
        );
    }
}

/// Listing workspaces publishes its filters in the order a client sends them,
/// and its success names the list schema.
#[test]
fn test_the_workspace_list_publishes_its_filters_and_its_schema() {
    let document = document();
    let list = operation(&document, LIST_WORKSPACES, "get");

    assert_eq!(
        parameter_names(&list),
        ["name", "starting_after", "limit"],
        "{list}"
    );
    let schema = list
        .pointer("/responses/200/content/application~1json/schema/$ref")
        .and_then(serde_json::Value::as_str);
    assert_eq!(
        schema,
        Some(format!("{SCHEMA_PREFIX}WorkspacesResponse").as_str()),
        "{list}"
    );
}
