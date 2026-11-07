//! The Definition's published shape, for the `OpenAPI` document only.
//!
//! Derived from the raw types the parser reads, so the document cannot name a
//! key the parser refuses or miss one it accepts, then narrowed by the bounds
//! each domain module lists against its own constants (see
//! [`crate::openapi`]). The wire crates point their Definition fields at
//! [`DefinitionSchema`]; it has no values, so nothing can construct one.

use std::borrow::Cow;

use utoipa::openapi::RefOr;
use utoipa::openapi::schema::Schema;
use utoipa::{PartialSchema, ToSchema};

use super::{DEFINITION_SCHEMA_VERSION, MAX_DESCRIPTION_CHARS, MAX_INSTRUCTIONS_CHARS};
use crate::config::raw;
use crate::openapi::{self, Bound, Components, Published, Text};

/// The fewest characters a description carries: `parse` refuses an empty one
/// as missing, whatever its garde bound allows.
const NOT_EMPTY: usize = 1;

/// A complete native fleet setup: its description, instructions and configuration.
#[derive(utoipa::ToSchema)]
#[schema(as = FleetDefinition)]
#[expect(
    dead_code,
    reason = "a schema description: utoipa reads the field types, and nothing builds or reads one"
)]
struct Shape {
    /// The Definition format version. Only `1` is accepted.
    schema_version: u8,
    /// Says what the fleet does, shown wherever it is listed. At most 32,768
    /// characters.
    description: String,
    /// The fleet's instructions, in Markdown, at most 32,768 characters.
    instructions_md: String,
    /// The fleet's configuration.
    config: raw::Document,
}

/// The published schema of a native Definition.
#[derive(Debug)]
pub enum DefinitionSchema {}

impl PartialSchema for DefinitionSchema {
    fn schema() -> RefOr<Schema> {
        let mut schema = Shape::schema();
        openapi::narrow(&mut schema, &root());
        schema
    }
}

impl ToSchema for DefinitionSchema {
    fn name() -> Cow<'static, str> {
        Shape::name()
    }

    fn schemas(schemas: &mut Components) {
        Shape::schemas(schemas);
        openapi::apply(schemas, &published());
    }
}

/// The Definition's own bounds. The parser's `Raw` refuses unknown keys.
pub(crate) fn root() -> Published {
    Published {
        component: Shape::name,
        closed: true,
        bounds: vec![
            (
                "schema_version",
                Bound::Range {
                    min: DEFINITION_SCHEMA_VERSION.into(),
                    max: DEFINITION_SCHEMA_VERSION.into(),
                },
            ),
            (
                "description",
                Bound::Text(Text::chars(NOT_EMPTY, MAX_DESCRIPTION_CHARS)),
            ),
            (
                "instructions_md",
                Bound::Text(Text::chars(0, MAX_INSTRUCTIONS_CHARS)),
            ),
        ],
    }
}

/// Every component a Definition references, and the bounds each publishes.
pub(crate) fn published() -> Vec<Published> {
    raw::openapi::published()
}
