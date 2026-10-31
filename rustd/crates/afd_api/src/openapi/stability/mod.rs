//! Each response field's stability class, as `x-stability` (REST §9).
//!
//! `utoipa` field extensions do not survive a reference-valued field. The
//! table keeps field classes together and tests every declared property in
//! the published document. A field left out is `stable`, which is the
//! guidelines' binding default.

use utoipa::openapi::extensions::Extensions;
use utoipa::openapi::schema::OneOf;
use utoipa::openapi::{OpenApi, RefOr, Schema};

/// The extension a field's class is published under.
const STABILITY: &str = "x-stability";

/// May change shape or go away without the deprecation clock.
const BETA: &str = "beta";

/// What a `beta` field's description says, so a reader of the rendered
/// reference sees the class the extension carries (REST §9).
const BETA_NOTE: &str = "Beta: this field may change shape before it is stable.";

/// The response fields published `beta`: each schema, with its properties.
///
/// The tool-call fields are read by the chat while its rendering is still
/// being built, and the runner that fills them is new; both may reshape them.
pub(super) const BETA_FIELDS: &[(&str, &[&str])] = &[
    ("NetworkPolicy", &["http_egress", "package_manager_domains"]),
    (
        "DefinitionNetwork",
        &["http_egress", "package_manager_domains"],
    ),
    ("EventDetail", &["tool_calls"]),
    (
        "ToolCallDetail",
        &[
            "call_id",
            "arguments",
            "truncated_arguments",
            "output",
            "output_line_count",
            "truncated",
        ],
    ),
    ("ToolCallRecordsStored", &["stored_count", "skipped_count"]),
    // The runner's schedules and messages verbs are new, and so is the Rust
    // runner that reads them; either may reshape what a reply carries.
    ("ScheduleRun", &["event_id"]),
    ("MessagePosted", &["delivered"]),
    // A schedule's view gained who made it and whether it retires after one
    // fire. `source` grew a third value with the fleet as an author, and both
    // are read by that same new runner.
    ("View", &["source", "once"]),
];

/// Publishes `fields` as `beta`, answering the entries the document lacks.
pub(super) fn declare_beta<'a>(
    document: &mut OpenApi,
    fields: &[(&'a str, &[&'a str])],
) -> Vec<(&'a str, &'a str)> {
    let mut missing = Vec::new();
    for &(schema, properties) in fields {
        for &property in properties {
            match property_of(document, schema, property).and_then(parts_of) {
                Some((extensions, description)) => {
                    extensions
                        .get_or_insert_with(Extensions::default)
                        .insert(STABILITY.to_owned(), BETA.into());
                    *description = Some(match description.take() {
                        Some(text) => format!("{text} {BETA_NOTE}"),
                        None => BETA_NOTE.to_owned(),
                    });
                }
                None => missing.push((schema, property)),
            }
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
    let field = object.properties.get_mut(property)?;
    // A single-reference union preserves validation while carrying field metadata.
    if let RefOr::Ref(reference) = field {
        let mut reference = reference.clone();
        let description =
            (!reference.description.is_empty()).then(|| std::mem::take(&mut reference.description));
        *field = Schema::OneOf(
            OneOf::builder()
                .item(reference)
                .description(description)
                .build(),
        )
        .into();
    }
    match field {
        RefOr::T(found) => Some(found),
        RefOr::Ref(_) => None,
    }
}

/// Where a field's schema keeps its extensions and its description.
///
/// The two shapes a response field takes here: an inline object, and the
/// `oneOf` a nullable reference becomes. Any other shape is answered as
/// missing, so the test naming every declared field fails on it.
fn parts_of(schema: &mut Schema) -> Option<(&mut Option<Extensions>, &mut Option<String>)> {
    match schema {
        Schema::Object(object) => Some((&mut object.extensions, &mut object.description)),
        Schema::OneOf(one_of) => Some((&mut one_of.extensions, &mut one_of.description)),
        _other_shape => None,
    }
}

#[cfg(test)]
mod tests;
