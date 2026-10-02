//! Each response field's stability class, as `x-stability` (REST §9).
//!
//! # Why this is derived and not annotated
//!
//! `utoipa` 5.5's `ToSchema` derive takes no extensions on a field, so the
//! class cannot be written beside the field it describes. The table below is
//! the next-closest place: one line per field that is not `stable`, checked by
//! a test that every line still names a field the document publishes. A field
//! left out is `stable`, which is the guidelines' binding default.

use utoipa::openapi::extensions::Extensions;
use utoipa::openapi::{OpenApi, RefOr, Schema};

/// The extension a field's class is published under.
const STABILITY: &str = "x-stability";

/// May change shape or go away without the deprecation clock.
const BETA: &str = "beta";

/// The response fields published `beta`, as `(schema, property)`.
///
/// `EventDetail.tool_calls` is read by the chat while its rendering is still
/// being built, and the runner that fills it is new; both may still reshape it.
pub(super) const BETA_FIELDS: &[(&str, &str)] = &[("EventDetail", "tool_calls")];

/// Publishes `fields` as `beta`, answering the entries the document lacks.
pub(super) fn declare_beta<'a>(
    document: &mut OpenApi,
    fields: &[(&'a str, &'a str)],
) -> Vec<(&'a str, &'a str)> {
    let mut missing = Vec::new();
    for &(schema, property) in fields {
        match property_of(document, schema, property).and_then(extensions_of) {
            Some(extensions) => {
                extensions
                    .get_or_insert_with(Extensions::default)
                    .insert(STABILITY.to_owned(), BETA.into());
            }
            None => missing.push((schema, property)),
        }
    }
    missing
}

/// One inline property of one named object schema.
fn property_of<'d>(
    document: &'d mut OpenApi,
    schema: &str,
    property: &str,
) -> Option<&'d mut Schema> {
    let components = document.components.as_mut()?;
    let RefOr::T(Schema::Object(object)) = components.schemas.get_mut(schema)? else {
        return None;
    };
    match object.properties.get_mut(property)? {
        RefOr::T(found) => Some(found),
        RefOr::Ref(_) => None,
    }
}

/// Where a field's schema keeps its extensions.
///
/// The two shapes a response field takes here: an inline object, and the
/// `oneOf` a nullable reference becomes. Any other shape is answered as
/// missing, so the test naming every declared field fails on it.
fn extensions_of(schema: &mut Schema) -> Option<&mut Option<Extensions>> {
    match schema {
        Schema::Object(object) => Some(&mut object.extensions),
        Schema::OneOf(one_of) => Some(&mut one_of.extensions),
        _other_shape => None,
    }
}

#[cfg(test)]
#[path = "stability/tests.rs"]
mod tests;
