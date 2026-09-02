//! The annotation and the signature describe the same body.
//!
//! # Why this exists
//!
//! `#[utoipa::path]` never sees the function it sits on. `body = X` is an
//! independent assertion, and when a handler returned the erased `Response`
//! there was nothing to check it against — which is how `store` came to
//! publish `SecretsResponse` and `list` to publish `StoredSecretResponse`,
//! each documenting the other's shape.
//!
//! A typed return gives the check something to stand on. [`StoredSecret`] is
//! named in the signature, so changing what the handler answers changes the
//! alias, and changing the alias fails this test unless the annotation moves
//! with it. That is the binding no static analysis could provide.

use utoipa::Path as _;

use super::StoredSecret;

/// A Rust type's short name, as utoipa spells it in a `$ref`.
fn schema_name<T: ?Sized>() -> &'static str {
    std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or_default()
        .split('<')
        .next()
        .unwrap_or_default()
}

/// The schema one documented response refers to.
fn documented(operation: &utoipa::openapi::path::Operation, status: &str) -> String {
    serde_json::to_value(operation).expect("the operation serializes")["responses"][status]
        ["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap_or("(none documented)")
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn the_documented_created_body_is_the_type_store_returns() {
    assert_eq!(
        documented(&super::__path_store::operation(), "201"),
        schema_name::<StoredSecret>(),
        "POST /v1/workspaces/{{workspace_id}}/secrets documents a 201 body \
         that is not the type the handler returns",
    );
}
