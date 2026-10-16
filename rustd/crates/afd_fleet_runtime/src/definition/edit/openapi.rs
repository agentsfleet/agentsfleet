//! The edit operations' published schemas.
//!
//! [`ConfigOp`] and [`InstructionEdit`] are read by serde as declared, but
//! their schemas need what a derive on them cannot write: utoipa takes only
//! literal numbers, and a variant of a tagged enum cannot be closed to unknown
//! keys. Each is described by a schema-only twin below, narrowed against the
//! constants its garde attributes read, and published under the real type, so
//! every wire crate that names the real type gets the narrowed schema.

use std::borrow::Cow;

use serde_json::Value;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::Schema;
use utoipa::{PartialSchema, ToSchema};

use super::{
    ConfigOp, InstructionEdit, MAX_CONFIG_OPS, MAX_INSTRUCTION_EDITS, MAX_INSTRUCTION_OFFSET,
    MAX_INSTRUCTIONS_CHARS, MAX_POINTER_CHARS,
};
use crate::openapi::{self, Bound, Published, Text};

/// The fewest characters a JSON Pointer carries.
const NOT_EMPTY: usize = 1;

/// The key a request carries its configuration operations under.
const CONFIG_OPS: &str = "config_ops";

/// The key a request carries its instruction edits under.
const INSTRUCTION_EDITS: &str = "instruction_edits";

/// Publishes the batch sizes `EditBatch` validates on the two edit lists of
/// `T`, the component whose `schema` is being narrowed.
///
/// A request that carries an edit batch inside its own keys, such as the
/// drawer's draft patch, calls this on its schema, so the published counts
/// are the constants the validator reads rather than a second spelling.
pub fn narrow_edit_batch<T: ToSchema>(schema: &mut RefOr<Schema>) {
    openapi::narrow(schema, &edit_batch(T::name));
}

/// The two edit lists' counts, on the component `component` names.
fn edit_batch(component: fn() -> Cow<'static, str>) -> Published {
    let list = |max| Bound::List {
        min: 0,
        max,
        entry: None,
    };
    Published {
        component,
        closed: false,
        bounds: vec![
            (CONFIG_OPS, list(MAX_CONFIG_OPS)),
            (INSTRUCTION_EDITS, list(MAX_INSTRUCTION_EDITS)),
        ],
    }
}

/// One JSON Patch operation on the setup: `add`, `replace` or `remove`.
#[derive(utoipa::ToSchema)]
#[schema(as = DefinitionConfigOp)]
#[serde(tag = "op", rename_all = "lowercase")]
#[expect(
    dead_code,
    reason = "a schema description: utoipa reads the variants, and nothing builds one"
)]
enum OpShape {
    /// Insert a member or array element below config.
    Add {
        /// JSON Pointer into mutable setup fields.
        path: String,
        /// The new value. The whole edited setup is validated before it is kept.
        value: Value,
    },
    /// Replace an existing member or array element below config, or the description.
    Replace {
        /// JSON Pointer into mutable setup fields.
        path: String,
        /// The new value.
        value: Value,
    },
    /// Remove an existing optional member or array element below config.
    Remove {
        /// JSON Pointer into mutable setup fields.
        path: String,
    },
}

/// A UTF-8 byte range against the exact base instructions.
#[derive(utoipa::ToSchema)]
#[schema(as = DefinitionInstructionEdit)]
#[expect(
    dead_code,
    reason = "a schema description: utoipa reads the fields, and nothing builds one"
)]
struct EditShape {
    /// Inclusive start in the unmodified base.
    start_byte: usize,
    /// Exclusive end in the unmodified base.
    end_byte: usize,
    /// Replacement text.
    text: String,
}

/// Every operation variant is closed, and its pointer bounded.
fn config_op() -> Published {
    Published {
        component: OpShape::name,
        closed: true,
        bounds: vec![(
            "path",
            Bound::Text(Text::chars(NOT_EMPTY, MAX_POINTER_CHARS)),
        )],
    }
}

/// A range never reaches past the largest instructions, nor its text.
fn instruction_edit() -> Published {
    let offset = Bound::Range {
        min: 0_u32.into(),
        max: MAX_INSTRUCTION_OFFSET.into(),
    };
    Published {
        component: EditShape::name,
        closed: true,
        bounds: vec![
            ("start_byte", offset.clone()),
            ("end_byte", offset),
            ("text", Bound::Text(Text::chars(0, MAX_INSTRUCTIONS_CHARS))),
        ],
    }
}

impl PartialSchema for ConfigOp {
    fn schema() -> RefOr<Schema> {
        let mut schema = OpShape::schema();
        openapi::narrow(&mut schema, &config_op());
        schema
    }
}

impl ToSchema for ConfigOp {
    fn name() -> Cow<'static, str> {
        OpShape::name()
    }
}

impl PartialSchema for InstructionEdit {
    fn schema() -> RefOr<Schema> {
        let mut schema = EditShape::schema();
        openapi::narrow(&mut schema, &instruction_edit());
        schema
    }
}

impl ToSchema for InstructionEdit {
    fn name() -> Cow<'static, str> {
        EditShape::name()
    }
}
