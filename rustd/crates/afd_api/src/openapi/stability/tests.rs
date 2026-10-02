#![expect(clippy::expect_used, reason = "a test asserts by panicking")]

use utoipa::openapi::schema::{AllOf, AnyOf, Array, Object, OneOf, Ref};
use utoipa::openapi::{ComponentsBuilder, OpenApi, OpenApiBuilder, Schema};

use super::{BETA, BETA_FIELDS, STABILITY, declare_beta, extensions_of};

/// The class a property publishes, read back from the serialized document.
fn published_class(document: &OpenApi, schema: &str, property: &str) -> Option<String> {
    let value = serde_json::to_value(document).expect("the document serializes");
    value
        .pointer(&format!(
            "/components/schemas/{schema}/properties/{property}/{STABILITY}"
        ))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[test]
fn every_beta_field_is_published_beta() {
    let document = crate::openapi::document();
    for &(schema, property) in BETA_FIELDS {
        assert_eq!(
            published_class(&document, schema, property).as_deref(),
            Some(BETA),
            "{schema}.{property}"
        );
    }
}

#[test]
fn a_field_the_document_lacks_is_answered_not_invented() {
    let object = Object::builder()
        .property("inline", Object::new())
        .property("shared", Ref::from_schema_name("Shared"))
        .build();
    let mut document = OpenApiBuilder::new()
        .components(Some(
            ComponentsBuilder::new()
                .schema("Holder", object)
                .schema("Listed", Array::new(Object::new()))
                .build(),
        ))
        .build();
    let missing = declare_beta(
        &mut document,
        &[
            ("Holder", "inline"),
            ("Holder", "shared"),
            ("Holder", "absent"),
            ("Listed", "items"),
            ("Nowhere", "field"),
        ],
    );
    assert_eq!(
        missing,
        [
            ("Holder", "shared"),
            ("Holder", "absent"),
            ("Listed", "items"),
            ("Nowhere", "field"),
        ]
    );
    assert_eq!(
        published_class(&document, "Holder", "inline").as_deref(),
        Some(BETA)
    );
    let mut bare = OpenApiBuilder::new().build();
    assert_eq!(declare_beta(&mut bare, &[("A", "b")]), [("A", "b")]);
}

#[test]
fn an_object_and_a_nullable_reference_carry_their_class() {
    let mut shapes = [Schema::Object(Object::new()), Schema::OneOf(OneOf::new())];
    for shape in &mut shapes {
        extensions_of(shape)
            .expect("a field of this shape can carry a class")
            .get_or_insert_with(Default::default)
            .insert(STABILITY.to_owned(), BETA.into());
        let value = serde_json::to_value(&*shape).expect("a schema serializes");
        assert_eq!(value[STABILITY], BETA, "{value}");
    }
    for other in [
        Schema::Array(Array::new(Object::new())),
        Schema::AllOf(AllOf::new()),
        Schema::AnyOf(AnyOf::new()),
    ] {
        let mut other = other;
        assert!(extensions_of(&mut other).is_none());
    }
}
