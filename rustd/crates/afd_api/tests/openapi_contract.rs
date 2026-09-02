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
#![cfg(all(feature = "test-util", feature = "openapi"))]

/// The verbs a `PathItem` can carry, as the document spells them.
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Statuses that carry no body by definition, so silence is correct.
const BODYLESS: [&str; 4] = ["204", "302", "303", "304"];

/// Where a schema reference points when it resolves.
const SCHEMA_PREFIX: &str = "#/components/schemas/";

/// The generated document, as the bytes that ship.
fn document() -> serde_json::Value {
    serde_json::to_value(afd_api::openapi::document())
        .expect("the generated document serializes")
}

/// Every `$ref` in the document, wherever it is nested.
fn references(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, nested) in fields {
                if key == "$ref" {
                    if let Some(target) = nested.as_str() {
                        found.push(target.to_owned());
                    }
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
            target.strip_prefix(SCHEMA_PREFIX).is_none_or(|name| {
                schemas.is_none_or(|schemas| !schemas.contains_key(name))
            })
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
