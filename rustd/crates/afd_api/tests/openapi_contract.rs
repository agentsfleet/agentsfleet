//! What the generated document promises about payloads, graded against itself.
//!
//! # Why this reads JSON and not the builder's types
//!
//! What ships is `public/openapi.json`. A reference that resolves in a
//! `BTreeMap` but not in the emitted document would be a passing test over an
//! artifact nobody reads, so the serialized form is the subject.
//!
//! # Why these two and not a body-by-body comparison
//!
//! The obvious gate is "the type an annotation names is the type the handler
//! serializes", and it is not derivable: `#[utoipa::path]` never sees the
//! return type, and an axum handler's body comes back through `IntoResponse`
//! several calls down. What IS decidable from the document alone is weaker and
//! still catches the whole class:
//!
//!   * a success that carries content and describes none types the call as
//!     returning nothing in every generated client
//!   * a `$ref` with no target is a dangling contract
//!
//! Both are mechanical, and the defects they catch are the ones a
//! route × method comparison is structurally blind to.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

/// The verbs a `PathItem` can carry, as the document spells them.
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Statuses that carry no body by definition, so silence is correct.
const BODYLESS: [&str; 4] = ["204", "302", "303", "304"];

/// Where a schema reference points when it resolves.
const SCHEMA_PREFIX: &str = "#/components/schemas/";

/// The verbs whose operations carry a document in.
const WRITES: [&str; 3] = ["post", "put", "patch"];

/// The writes that read no body, and why each is honest about it.
///
/// Every other write names what it reads, so a client can send it.
const BODILESS_WRITES: [(&str, &str, &str); 4] = [
    (
        "post",
        "/v1/runners/me/leases",
        "the poll reads nothing: one wire shape, no negotiation",
    ),
    (
        "post",
        "/v1/connectors/{provider}/callback",
        "the provider answers in the query string",
    ),
    (
        "post",
        "/v1/workspaces/{workspace_id}/connectors/{provider}/connect",
        "the route starts a round-trip and takes nothing",
    ),
    (
        "post",
        "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}/sync",
        "the verb is the whole request",
    ),
];

/// The generated document, as the bytes that ship.
fn document() -> serde_json::Value {
    serde_json::to_value(afd_api::openapi::document()).expect("the generated document serializes")
}

/// Every `$ref` in the document, wherever it is nested.
fn references(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, nested) in fields {
                if key == "$ref"
                    && let Some(target) = nested.as_str()
                {
                    found.push(target.to_owned());
                }
                references(nested, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                references(item, found);
            }
        }
        _ => {}
    }
}

/// A success that carries content describes the content it carries.
///
/// A 2xx with no `content` types the call as returning nothing, so a generated
/// client hands its caller a unit value and the real body is unreachable
/// without hand-editing. The bodyless statuses are held out because silence is
/// the correct description for them.
#[test]
fn test_every_content_bearing_success_describes_its_body() {
    let document = document();
    let mut silent = Vec::new();

    let paths = document.get("paths").and_then(serde_json::Value::as_object);
    for (path, item) in paths.into_iter().flatten() {
        for method in METHODS {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let responses = operation
                .get("responses")
                .and_then(serde_json::Value::as_object);
            for (code, response) in responses.into_iter().flatten() {
                if !code.starts_with('2') && !code.starts_with('3') {
                    continue;
                }
                if BODYLESS.contains(&code.as_str()) {
                    continue;
                }
                if response.get("content").is_none() {
                    silent.push(format!("{} {path} {code}", method.to_uppercase()));
                }
            }
        }
    }

    assert!(
        silent.is_empty(),
        "a success describes no body, so a generated client types the call as \
         returning nothing ({} of them):\n  {}",
        silent.len(),
        silent.join("\n  "),
    );
}

/// Every reference the document makes resolves inside the document.
///
/// A `$ref` naming a schema that was never registered is a contract a client
/// generator cannot compile. This is the failure mode of annotating `body =
/// SomeType` on a type that never derives `ToSchema`: the reference is emitted,
/// the target is not.
#[test]
fn test_every_reference_resolves() {
    let document = document();
    let mut found = Vec::new();
    references(&document, &mut found);

    let schemas = document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(serde_json::Value::as_object);

    let mut dangling: Vec<String> = found
        .iter()
        .filter(|target| {
            target
                .strip_prefix(SCHEMA_PREFIX)
                .is_none_or(|name| schemas.is_none_or(|schemas| !schemas.contains_key(name)))
        })
        .cloned()
        .collect();
    dangling.sort_unstable();
    dangling.dedup();

    assert!(
        !found.is_empty(),
        "the document makes no references at all; this gate would pass against \
         an empty document"
    );
    assert!(
        dangling.is_empty(),
        "a reference names a schema the document does not carry:\n  {}",
        dangling.join("\n  "),
    );
}

/// The lease's egress rules and the runner's posture are two schemas.
///
/// Both Rust types are named `NetworkPolicy`, after the two Zig types they
/// port, and utoipa keys components by name alone. Before the aliases the
/// document said a run's egress rules were a three-word string, and every
/// reference still resolved.
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

/// A tar is binary bytes under its own media type, and a stream is events.
///
/// utoipa reads a byte slice as an array of integers, which a generated client
/// parses as JSON and fails on the first byte of a tar; the document names a
/// binary string instead. Neither the body gate nor the reference gate would
/// notice that reverting, nor a stream published under `application/json`.
#[test]
fn test_the_tar_and_the_streams_publish_under_their_own_media_types() {
    let document = document();
    let media_types = |path: &str| -> Option<Vec<String>> {
        document
            .get("paths")?
            .get(path)?
            .get("get")?
            .get("responses")?
            .get("200")?
            .get("content")?
            .as_object()
            .map(|content| content.keys().cloned().collect())
    };
    let tar = document
        .get("paths")
        .and_then(|paths| paths.get("/v1/runners/me/bundles/{content_hash}"))
        .and_then(|item| item.get("get"))
        .and_then(|operation| operation.get("responses"))
        .and_then(|responses| responses.get("200"))
        .and_then(|response| response.get("content"))
        .and_then(|content| content.get("application/x-tar"))
        .and_then(|media| media.get("schema"))
        .and_then(|schema| schema.get("$ref"))
        .and_then(serde_json::Value::as_str)
        .and_then(|target| target.strip_prefix(SCHEMA_PREFIX))
        .and_then(|name| document.get("components")?.get("schemas")?.get(name));

    assert_eq!(
        media_types("/v1/runners/me/bundles/{content_hash}"),
        Some(vec!["application/x-tar".to_owned()])
    );
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
            media_types(path),
            Some(vec!["text/event-stream".to_owned()]),
            "{path}"
        );
    }
}

/// Every write that reads a body says what it reads.
///
/// A POST, PUT or PATCH with no `requestBody` is typed by every generated
/// client as taking nothing, so the caller has no way to send the document the
/// handler parses. The four that genuinely take nothing are listed with their
/// reason, so a fifth cannot join them by omission.
#[test]
fn test_every_write_names_the_body_it_reads() {
    let document = document();
    let mut mute = Vec::new();

    let paths = document.get("paths").and_then(serde_json::Value::as_object);
    for (path, item) in paths.into_iter().flatten() {
        for method in WRITES {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let excused = BODILESS_WRITES
                .iter()
                .any(|(verb, template, _reason)| *verb == method && template == path);
            if operation.get("requestBody").is_none() && !excused {
                mute.push(format!("{} {path}", method.to_uppercase()));
            }
        }
    }

    assert!(
        mute.is_empty(),
        "a write names no body, so a generated client cannot send one ({} of them):\n  {}",
        mute.len(),
        mute.join("\n  "),
    );
}
