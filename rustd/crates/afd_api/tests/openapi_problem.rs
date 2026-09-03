//! What the generated document promises about refusals, graded against the
//! envelope that writes them.
//!
//! # Why two gates and not one
//!
//! The first reads the shipped JSON: every 4xx and 5xx names the problem body
//! under its media type, so a generated client can reach `error_code` on a
//! refusal rather than being handed nothing. The second holds that schema to
//! the writer: the fields the document promises are the fields
//! `ProblemResponse` puts on the wire, required where every refusal carries
//! them and optional where only one status does. Either alone is a document
//! that agrees with itself and not with the daemon.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

use std::collections::BTreeSet;

use afd_api::{CONTENT_TYPE_PROBLEM_JSON, ProblemResponse};
use afd_core::error_code;
use afd_http::envelope::Resolution;
use afd_http::openapi::problem::ProblemBody;
use axum::body::to_bytes;
use axum::response::IntoResponse;
use utoipa::ToSchema as _;

/// The verbs a `PathItem` can carry, as the document spells them.
///
/// All eight, not the five this daemon mounts: the pass under test walks every
/// one, and a test that walked fewer would share its blind spot rather than
/// grade it.
const METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

/// Where a schema reference points when it resolves.
const SCHEMA_PREFIX: &str = "#/components/schemas/";

/// The refusals that carry a body of their own, and why each is honest.
///
/// Every other 4xx and 5xx is the envelope writer's, so a second entry here
/// needs a handler that answers an error status without it.
const SELF_DESCRIBED: [(&str, &str, &str, &str); 1] = [(
    "get",
    "/readyz",
    "503",
    "the probe answers its readiness report, not a refusal",
)];

/// The generated document, as the bytes that ship.
fn document() -> serde_json::Value {
    serde_json::to_value(afd_api::openapi::document()).expect("the generated document serializes")
}

/// The keys a rendered refusal puts on the wire.
async fn wire_keys(problem: ProblemResponse) -> BTreeSet<String> {
    let response = problem.into_response();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a problem body is small and in memory");
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).expect("the envelope must be valid JSON");
    json.as_object()
        .expect("the envelope is an object")
        .keys()
        .cloned()
        .collect()
}

/// Every refusal names the problem body under its media type.
///
/// A 4xx with no `content` types the refusal as nothing in every generated
/// client, so the code a caller switches on is unreachable without parsing
/// the bytes by hand. The one refusal that carries a different body is held
/// out by name, with its reason.
#[test]
fn test_every_refusal_names_the_problem_body() {
    let document = document();
    let expected = format!("{SCHEMA_PREFIX}{}", ProblemBody::name());
    let mut wrong = Vec::new();
    let mut graded = 0_usize;

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
                // The production predicate, not a second spelling of it: a
                // test that decided "refusal" its own way would pass while the
                // two disagreed about `default` and the range keys.
                if !afd_api::openapi::is_a_refusal(code) {
                    continue;
                }
                graded += 1;
                let excused = SELF_DESCRIBED.iter().any(|(verb, template, status, _)| {
                    *verb == method && template == path && status == code
                });
                if excused {
                    if response.get("content").is_none() {
                        wrong.push(format!(
                            "{} {path} {code}: held out and yet silent",
                            method.to_uppercase()
                        ));
                    }
                    continue;
                }
                let target = response
                    .get("content")
                    .and_then(|content| content.get(CONTENT_TYPE_PROBLEM_JSON))
                    .and_then(|media| media.get("schema"))
                    .and_then(|schema| schema.get("$ref"))
                    .and_then(serde_json::Value::as_str);
                if target != Some(expected.as_str()) {
                    wrong.push(format!("{} {path} {code}", method.to_uppercase()));
                }
            }
        }
    }

    assert!(graded > 0, "the document declares no refusals at all");
    assert!(
        successes_carrying_a_problem_body(&document).is_empty(),
        "a success carries a refusal body, so the pass has widened past the \
         statuses the envelope writer actually answers: {:?}",
        successes_carrying_a_problem_body(&document),
    );
    assert!(
        wrong.is_empty(),
        "a refusal does not name the problem body, so a generated client types it \
         as nothing ({} of them):\n  {}",
        wrong.len(),
        wrong.join("\n  "),
    );
}

/// Every success or redirect that wrongly carries a refusal body.
///
/// The negative half of the gate above. Without it a pass widened to every
/// parseable status would stamp `application/problem+json` onto all thirteen
/// of the document's 204s and the positive assertion would still be green.
fn successes_carrying_a_problem_body(document: &serde_json::Value) -> Vec<String> {
    let mut wrong = Vec::new();
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
                if afd_api::openapi::is_a_refusal(code) {
                    continue;
                }
                if response
                    .get("content")
                    .and_then(|content| content.get(CONTENT_TYPE_PROBLEM_JSON))
                    .is_some()
                {
                    wrong.push(format!("{} {path} {code}", method.to_uppercase()));
                }
            }
        }
    }
    wrong
}

/// The schema the document publishes is the envelope the daemon writes.
///
/// Required is what every refusal carries; optional is what one status adds.
/// A field the writer sends and the schema omits is invisible to a generated
/// client, and a field the schema promises and the writer never sends is a
/// null the client did not expect.
#[tokio::test]
async fn test_the_problem_schema_is_the_envelope_the_daemon_writes() {
    let document = document();
    let schema = document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(|schemas| schemas.get(ProblemBody::name().as_ref()))
        .expect("the document carries the problem body");
    let promised: BTreeSet<String> = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("the problem body has properties")
        .keys()
        .cloned()
        .collect();
    let required: BTreeSet<String> = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .expect("the problem body names its required fields")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();

    let base = wire_keys(ProblemResponse::new(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
    ))
    .await;
    let conflict = wire_keys(ProblemResponse::conflict(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
        "paused",
    ))
    .await;
    let precondition = wire_keys(ProblemResponse::precondition_failed(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
        "W/\"v7\"",
    ))
    .await;
    let mut sent: BTreeSet<String> = BTreeSet::new();
    for problem in afd_core::problem::entries() {
        sent.extend(wire_keys(ProblemResponse::new(problem.code(), "d", "r")).await);
    }
    let standing = wire_keys(ProblemResponse::already_resolved(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
        Resolution {
            gate_id: "g".to_owned(),
            action_id: "a".to_owned(),
            outcome: "approved".to_owned(),
            resolved_at: 1,
            resolved_by: "someone".to_owned(),
        },
    ))
    .await;
    let short_a_credential = wire_keys(ProblemResponse::missing_secrets(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
        vec!["github".to_owned()],
    ))
    .await;
    sent.extend(conflict.iter().cloned());
    sent.extend(precondition.iter().cloned());
    sent.extend(short_a_credential.iter().cloned());
    sent.extend(standing.iter().cloned());

    assert_eq!(required, base, "required is what every refusal carries");
    assert_eq!(
        promised, sent,
        "the schema promises the fields the writer sends, and no others"
    );
    assert!(conflict.contains("current_state") && precondition.contains("etag"));
    assert!(short_a_credential.contains("missing_secrets"));
    for member in [
        "gate_id",
        "action_id",
        "outcome",
        "resolved_at",
        "resolved_by",
    ] {
        assert!(
            standing.contains(member),
            "{member} is what the dashboard renders off an answered gate"
        );
    }
}
