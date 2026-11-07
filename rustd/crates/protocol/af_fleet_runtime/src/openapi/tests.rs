#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use utoipa::openapi::schema::{
    AdditionalProperties, ArrayBuilder, ObjectBuilder, OneOfBuilder, Schema, Type,
};
use utoipa::openapi::{Object, Ref, RefOr};
use utoipa::{PartialSchema as _, ToSchema as _};

use super::{Bound, Components, Published, Text, narrow, objects};
use crate::definition::DefinitionSchema;
use crate::definition::schema::{published, root};

/// The schemas a Definition publishes, the Definition itself included.
fn definition_components() -> Components {
    let mut components = vec![(
        DefinitionSchema::name().into_owned(),
        DefinitionSchema::schema(),
    )];
    DefinitionSchema::schemas(&mut components);
    components
}

/// Whether any object in `schema` lists `key` among its properties.
fn lists(schema: &mut RefOr<Schema>, key: &str) -> bool {
    objects(schema)
        .into_iter()
        .any(|object| object.properties.contains_key(key))
}

/// A bound whose key names no property would publish nothing and say nothing,
/// so a renamed field would quietly lose its bound.
#[test]
fn every_published_bound_names_a_property_its_component_has() {
    let mut components = definition_components();
    let every: Vec<Published> = published().into_iter().chain([root()]).collect();
    for entry in &every {
        let name = (entry.component)();
        let (_, schema) = components
            .iter_mut()
            .find(|(found, _)| *found == name)
            .unwrap_or_else(|| panic!("`{name}` is not among the Definition's components"));
        for (key, _bound) in &entry.bounds {
            assert!(
                lists(schema, key),
                "`{name}` publishes a bound on `{key}`, which it has no property for"
            );
        }
    }
}

/// A published entry that closes its objects and bounds `key` as `bound`.
fn closing(key: &'static str, bound: Bound) -> Published {
    Published {
        component: DefinitionSchema::name,
        closed: true,
        bounds: vec![(key, bound)],
    }
}

/// An object with one string property and one list property.
fn object() -> Object {
    ObjectBuilder::new()
        .property(TEXT, ObjectBuilder::new().schema_type(Type::String))
        .property(
            LIST,
            ArrayBuilder::new().items(ObjectBuilder::new().schema_type(Type::String)),
        )
        .build()
}

/// The string property's key.
const TEXT: &str = "text";
/// The list property's key.
const LIST: &str = "list";
/// A component some other module publishes.
const ELSEWHERE: &str = "Elsewhere";

#[test]
fn a_reference_or_an_array_at_the_top_is_left_as_it_is() {
    let published = closing(TEXT, Bound::Text(Text::chars(1, 2)));
    let mut reference = RefOr::Ref(Ref::from_schema_name(ELSEWHERE));
    narrow(&mut reference, &published);
    assert!(matches!(reference, RefOr::Ref(_)));

    let mut array = RefOr::T(Schema::Array(ArrayBuilder::new().build()));
    narrow(&mut array, &published);
    let RefOr::T(Schema::Array(array)) = array else {
        panic!("narrowing never changes a schema's kind");
    };
    assert_eq!(array.max_items, None);
}

#[test]
fn a_one_of_closes_its_object_variants_and_skips_its_references() {
    let mut schema = RefOr::T(Schema::OneOf(
        OneOfBuilder::new()
            .item(object())
            .item(Ref::from_schema_name(ELSEWHERE))
            .build(),
    ));
    narrow(&mut schema, &closing(TEXT, Bound::Text(Text::chars(1, 2))));
    let RefOr::T(Schema::OneOf(one_of)) = schema else {
        panic!("narrowing never changes a schema's kind");
    };
    let [RefOr::T(Schema::Object(variant)), RefOr::Ref(_)] = one_of.items.as_slice() else {
        panic!("the variants keep their order and kind");
    };
    assert!(matches!(
        variant.additional_properties.as_deref(),
        Some(AdditionalProperties::FreeForm(false))
    ));
    let Some(RefOr::T(Schema::Object(text))) = variant.properties.get(TEXT) else {
        panic!("the string property survives");
    };
    assert_eq!((text.min_length, text.max_length), (Some(1), Some(2)));
}

#[test]
fn a_bound_of_the_wrong_kind_changes_nothing() {
    let mut schema = RefOr::T(Schema::Object(object()));
    let published = Published {
        component: DefinitionSchema::name,
        closed: false,
        bounds: vec![
            (LIST, Bound::Text(Text::chars(1, 2))),
            (
                TEXT,
                Bound::List {
                    min: 1,
                    max: 2,
                    entry: None,
                },
            ),
        ],
    };
    narrow(&mut schema, &published);
    let RefOr::T(Schema::Object(narrowed)) = schema else {
        panic!("narrowing never changes a schema's kind");
    };
    assert!(narrowed.additional_properties.is_none());
    let Some(RefOr::T(Schema::Array(list))) = narrowed.properties.get(LIST) else {
        panic!("the list property survives");
    };
    assert_eq!((list.min_items, list.max_items), (None, None));
    let Some(RefOr::T(Schema::Object(text))) = narrowed.properties.get(TEXT) else {
        panic!("the string property survives");
    };
    assert_eq!(text.max_length, None);
}
