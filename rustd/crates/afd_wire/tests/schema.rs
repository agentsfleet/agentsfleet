//! The three shapes a schema derive over THIS crate's types could have failed on.
//!
//! # Why a spike, and why it is over real types
//!
//! The external review that settled the derive left exactly one papercut open:
//! a standalone top-level `Cow<'a, str>` as a reusable component is suspect,
//! because the impl is `impl<'a, T: ToSchema + Clone> ToSchema for Cow<'a, T>`
//! with `T: Sized` implied. As a FIELD it is documented to work. Borrowed wire
//! types were never the problem — utoipa 5 removed the trait's lifetime
//! parameter, and its own documentation derives on a `Cow`-carrying struct.
//!
//! What IS a real difference is `RawValue`. It has no `ToSchema` impl and can
//! have none: it is an unparsed slice whose serialized form is an arbitrary
//! JSON value. That is precisely what `value_type` is for, and this file is
//! what proves the override produces the shape the contract publishes.
//!
//! The types below are the crate's own, not fixtures. A spike type invented to
//! be spiked would be dead code the moment it passed (RULE NDC), and it would
//! prove a shape nothing serves (RULE TVR).
#![cfg(feature = "openapi")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::preference::PreferencesResponse;
use afd_wire::secret::{StoreSecretRequest, StoredSecretResponse};
use afd_wire::{policy, runner};
use utoipa::{PartialSchema, ToSchema};

/// The schema `T` emits, as JSON a test can read field by field.
fn schema_of<T: PartialSchema>() -> serde_json::Value {
    serde_json::to_value(T::schema()).expect("a derived schema serializes")
}

/// A borrowed field is a string, and an opaque body is an object.
///
/// One type carries both: `name` is `Cow<'a, str>` behind `#[serde(borrow)]`
/// and `data` is `&'a RawValue`. If the lifetime had been a wall this would not
/// compile; if the override were missing the crate would not compile either,
/// so what is left to check is that each lands on the RIGHT shape.
#[test]
fn a_borrowed_field_is_a_string_and_an_opaque_body_is_an_object() {
    let schema = schema_of::<StoreSecretRequest<'_>>();
    let properties = &schema["properties"];

    assert_eq!(
        properties["name"]["type"], "string",
        "a borrowed `Cow<str>` field must publish as a string, not as a \
         reference to a component: {schema}"
    );
    assert_eq!(
        properties["data"]["type"], "object",
        "an opaque `&RawValue` body must publish as an object through its \
         `value_type` override: {schema}"
    );
    assert!(
        schema["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|field| field == "data")),
        "a non-optional field stays required: {schema}"
    );
}

/// A type whose only field is borrowed still emits its own object schema.
///
/// The standalone-`Cow` papercut, asked the way this crate can actually meet
/// it: never as a top-level component, always as a field of one.
#[test]
fn a_type_carrying_only_borrowed_text_still_emits_an_object() {
    let schema = schema_of::<StoredSecretResponse<'_>>();

    assert_eq!(schema["type"], "object", "expected an object: {schema}");
    assert_eq!(
        schema["properties"]["name"]["type"], "string",
        "the borrowed name must publish as a string: {schema}"
    );
}

/// A map of borrowed keys to unparsed values publishes as a free-form object.
///
/// The third suspect shape, and the one the preferences bag actually is.
#[test]
fn a_map_of_borrowed_keys_to_raw_values_publishes_as_an_object() {
    let schema = schema_of::<PreferencesResponse<'_>>();

    assert_eq!(
        schema["properties"]["prefs"]["type"], "object",
        "the preferences bag must publish as an object: {schema}"
    );
}

/// The two `NetworkPolicy` types publish under two names.
///
/// utoipa keys components by name alone, so two types spelled alike collapse
/// into one entry and whichever registered second wins. The lease's egress
/// rules were published as the runner's three-word posture that way, and the
/// reference-resolves gate could not see it: the name resolved.
#[test]
fn the_two_network_policies_publish_under_two_names() {
    let rules = policy::NetworkPolicy::name();
    let posture = runner::NetworkPolicy::name();

    assert_ne!(rules, posture, "two shapes under one component name");
    assert_eq!(
        schema_of::<policy::NetworkPolicy<'_>>()["type"],
        "object",
        "the egress rules are an object"
    );
    assert_eq!(
        schema_of::<runner::NetworkPolicy>()["type"],
        "string",
        "the posture is an enum"
    );
}
