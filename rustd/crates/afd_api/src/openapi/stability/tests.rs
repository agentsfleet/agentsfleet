#![expect(clippy::expect_used, reason = "a test asserts by panicking")]

use utoipa::openapi::schema::{AllOf, AnyOf, Array, Object, OneOf, Ref};
use utoipa::openapi::{ComponentsBuilder, OpenApi, OpenApiBuilder, Schema};

use super::{BETA, BETA_FIELDS, BETA_NOTE, STABILITY, declare_beta, parts_of};

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
    for &(schema, properties) in BETA_FIELDS {
        for &property in properties {
            assert_eq!(
                published_class(&document, schema, property).as_deref(),
                Some(BETA),
                "{schema}.{property}"
            );
            let value = serde_json::to_value(&document).expect("the document serializes");
            let description = value
                .pointer(&format!(
                    "/components/schemas/{schema}/properties/{property}/description"
                ))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            assert!(
                description.ends_with(BETA_NOTE),
                "{schema}.{property}: {description}"
            );
        }
    }
}

#[test]
fn http_destination_settings_are_published_beta() {
    let document = crate::openapi::document();
    let value = serde_json::to_value(&document).expect("document serializes");
    for schema in ["NetworkPolicy", "DefinitionNetwork"] {
        for property in ["http_egress", "package_manager_domains"] {
            assert_eq!(
                published_class(&document, schema, property).as_deref(),
                Some("beta"),
                "{schema}.{property}: {:?}",
                value.pointer(&format!(
                    "/components/schemas/{schema}/properties/{property}"
                ))
            );
        }
    }
}

#[test]
fn a_field_the_document_lacks_is_answered_not_invented() {
    let object = Object::builder().property("inline", Object::new()).build();
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
            ("Holder", &["inline", "absent"]),
            ("Listed", &["items"]),
            ("Nowhere", &["field"]),
        ],
    );
    assert_eq!(
        missing,
        [
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
    assert_eq!(declare_beta(&mut bare, &[("A", &["b"])]), [("A", "b")]);
}

#[test]
fn a_referenced_field_keeps_its_target_and_carries_its_class() {
    let reference = Ref::from_schema_name("Shared");
    let object = Object::builder().property("shared", reference).build();
    let mut document = OpenApiBuilder::new()
        .components(Some(
            ComponentsBuilder::new()
                .schema("Holder", object)
                .schema("Shared", Object::new())
                .build(),
        ))
        .build();
    assert!(declare_beta(&mut document, &[("Holder", &["shared"])]).is_empty());
    assert_eq!(
        published_class(&document, "Holder", "shared").as_deref(),
        Some("beta")
    );
    let encoded = serde_json::to_value(&document).expect("document serializes");
    assert_eq!(
        encoded.pointer("/components/schemas/Holder/properties/shared/oneOf/0/$ref"),
        Some(&serde_json::json!("#/components/schemas/Shared"))
    );
}

#[test]
fn an_object_and_a_nullable_reference_carry_their_class() {
    let mut shapes = [Schema::Object(Object::new()), Schema::OneOf(OneOf::new())];
    for shape in &mut shapes {
        let (extensions, description) =
            parts_of(shape).expect("a field of this shape can carry a class");
        extensions
            .get_or_insert_with(Default::default)
            .insert(STABILITY.to_owned(), BETA.into());
        *description = Some(BETA_NOTE.to_owned());
        let value = serde_json::to_value(&*shape).expect("a schema serializes");
        assert_eq!(
            value.get(STABILITY).and_then(serde_json::Value::as_str),
            Some(BETA),
            "{value}"
        );
    }
    for other in [
        Schema::Array(Array::new(Object::new())),
        Schema::AllOf(AllOf::new()),
        Schema::AnyOf(AnyOf::new()),
    ] {
        let mut other = other;
        assert!(parts_of(&mut other).is_none());
    }
}
