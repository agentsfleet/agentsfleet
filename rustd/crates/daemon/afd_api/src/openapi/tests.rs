//! The merged document preserves route guards, refusal bodies and unique schemas.

use utoipa::openapi::path::{OperationBuilder, PathItem};
use utoipa::openapi::{HttpMethod, OpenApi, PathsBuilder, Ref, ResponsesBuilder};

use super::{
    describe_every_refusal_as_a_problem, is_a_refusal, require_the_credential_each_route_guards,
};

/// No component name is published by two different types.
///
/// utoipa keys components by name, and a second type under a taken name
/// replaces the first without a word: every operation that named the lost
/// type then points at the other one's shape. The merged document cannot
/// show it, so this reads one document per handler from every plane
/// (`afd_http::plane_collector!`) plus this crate's own `Root`, where each
/// type a handler reaches publishes its own schema. One type reached twice
/// publishes the same schema twice; a name with two different schemas is
/// two types. Two colliding types every handler reaches together collapse
/// inside each document before this sees them; that costs one wrong shape
/// in the API reference, and is accepted.
#[cfg(feature = "test-util")]
#[test]
fn every_component_name_is_one_type() {
    use utoipa::OpenApi as _;

    let documents = std::iter::once(super::Root::openapi())
        .chain(afd_api_tenant::openapi::handler_documents())
        .chain(afd_api_runner::openapi::handler_documents())
        .chain(afd_api_operator::openapi::handler_documents())
        .chain(afd_api_ingress::openapi::handler_documents());
    let (read, collisions) = component_collisions(documents);
    assert!(
        read > 0,
        "no handler published a component; the audit read nothing"
    );
    assert!(
        collisions.is_empty(),
        "{collisions:?} each published by two different types; utoipa keeps one and drops the other"
    );
}

/// The audit names a component two documents publish with different
/// schemas, and passes one published the same way twice: an audit that
/// never compared would pass every tree.
#[cfg(feature = "test-util")]
#[test]
fn the_audit_names_two_types_under_one_name() {
    use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};
    use utoipa::openapi::{ComponentsBuilder, OpenApiBuilder};

    let publishing = |kind: Type| {
        let schema = Schema::Object(ObjectBuilder::new().schema_type(kind).build());
        OpenApiBuilder::new()
            .components(Some(
                ComponentsBuilder::new().schema("Policy", schema).build(),
            ))
            .build()
    };

    let (_, clashing) = component_collisions([publishing(Type::String), publishing(Type::Integer)]);
    let (read, agreeing) =
        component_collisions([publishing(Type::String), publishing(Type::String)]);

    assert_eq!(clashing, ["Policy"]);
    assert_eq!((read, agreeing.len()), (1, 0));
}

/// How many component names `documents` publish, and every name two of
/// them publish with different schemas, sorted, once each.
#[cfg(feature = "test-util")]
fn component_collisions(documents: impl IntoIterator<Item = OpenApi>) -> (usize, Vec<String>) {
    use std::collections::BTreeMap;
    use std::collections::btree_map::Entry;

    let mut published = BTreeMap::new();
    let mut collisions = Vec::new();
    for document in documents {
        for (name, schema) in document.components.into_iter().flat_map(|c| c.schemas) {
            match published.entry(name) {
                Entry::Vacant(first) => {
                    first.insert(schema);
                }
                Entry::Occupied(first) if *first.get() != schema => {
                    collisions.push(first.key().clone());
                }
                Entry::Occupied(_) => {}
            }
        }
    }
    collisions.sort_unstable();
    collisions.dedup();
    (published.len(), collisions)
}

/// A document missing every route is left as it is, not invented into.
#[test]
fn a_route_the_document_lacks_is_left_to_the_coverage_gate() {
    let mut empty = OpenApi::default();
    require_the_credential_each_route_guards(&mut empty);
    assert!(empty.paths.paths.is_empty());
}

/// A refusal that names a shared response describes itself, and a
/// success is never given a problem body.
#[test]
fn a_shared_or_successful_response_is_left_as_it_is() {
    let responses = ResponsesBuilder::new()
        .response("404", Ref::new("#/components/responses/Shared"))
        .response("200", utoipa::openapi::Response::new("ok"))
        .build();
    let operation = OperationBuilder::new().responses(responses).build();
    let paths = PathsBuilder::new()
        .path("/probe", PathItem::new(HttpMethod::Get, operation))
        .build();
    let mut document = OpenApi::new(utoipa::openapi::Info::new("t", "1"), paths);
    let before = serde_json::to_value(&document).ok();
    describe_every_refusal_as_a_problem(&mut document);
    assert_eq!(serde_json::to_value(&document).ok(), before);
}

#[test]
fn every_refusal_spelling_is_a_refusal_and_nothing_else_is() {
    for refusal in ["4XX", "5XX", "default", "404", "503"] {
        assert!(is_a_refusal(refusal), "{refusal}");
    }
    for answer in ["200", "202", "301", "2XX", "not-a-status"] {
        assert!(!is_a_refusal(answer), "{answer}");
    }
}
