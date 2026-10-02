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
//!
//! The few schemas and media types whose shape is itself the promise are
//! `openapi_contract_schemas.rs`, which reads the same document.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

/// The operation this build must not over-promise for.
const CONNECTOR_INGRESS: &str = "ingest_connector_webhook";

/// The verbs a `PathItem` can carry, as the document spells them.
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Statuses that carry no body by definition, so silence is correct.
const BODYLESS: [&str; 4] = ["204", "302", "303", "304"];

/// Where a schema reference points when it resolves.
pub(crate) const SCHEMA_PREFIX: &str = "#/components/schemas/";

/// The verbs whose operations carry a document in.
const WRITES: [&str; 3] = ["post", "put", "patch"];

/// The writes that read no body, and why each is honest about it.
///
/// Every other write names what it reads, so a client can send it.
const BODILESS_WRITES: [(&str, &str, &str); 6] = [
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
    (
        "post",
        "/v1/users/me/invites/{invite_id}/accept",
        "the invite and the signed-in person are the whole request",
    ),
    (
        "post",
        "/v1/tenants/me/invites/{invite_id}/send",
        "the invite is the whole request",
    ),
];

/// The generated document, as the bytes that ship.
pub(crate) fn document() -> serde_json::Value {
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

/// The ingress description does not promise a writer this build has no home for.
///
/// `webhook/app_route.rs` says in its own module note that the repair-evidence
/// writers are unported and that `deployment_status` is a documented gap. Its
/// generated description said the opposite — that repair pull requests and
/// workflow results update repair evidence, and that a terminal
/// `deployment_status` records the deployed commit and schedules verification
/// fleets. Both sentences shipped in `public/openapi.json`, which is the one an
/// integrator reads and the only one they can act on.
///
/// Graded as the ABSENCE of the promise rather than the presence of a
/// replacement, because what must not happen is a reader budgeting for evidence
/// that never arrives. The wording is free to improve; the claim is not free to
/// come back.
#[test]
fn test_the_ingress_description_promises_no_unported_writer() {
    let document = document();
    // Found by operation id, not by path: several routes live under
    // `/v1/ingress/`, and the cron one matched a path filter first.
    let described = document
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("the document describes its paths")
        .values()
        .filter_map(|item| item.get("post"))
        .find(|post| {
            post.get("operationId")
                .is_some_and(|id| id == CONNECTOR_INGRESS)
        })
        .and_then(|post| post.get("description"))
        .and_then(serde_json::Value::as_str)
        .expect("the connector ingress route describes its POST")
        .to_owned();

    for promise in [
        "update repair evidence",
        "records the deployed commit",
        "schedules eligible verification",
    ] {
        assert!(
            !described.contains(promise),
            "the ingress description promises `{promise}`, which this build has \
             no writer for: {described}"
        );
    }
    assert!(
        described.contains("repair-evidence writer"),
        "the description must name the gap rather than leave it silent: {described}"
    );
}
