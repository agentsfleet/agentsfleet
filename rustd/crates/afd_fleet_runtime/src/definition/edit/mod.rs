//! Small optimistic edits validate one complete replacement and never mutate the base.

#[cfg(feature = "openapi")]
mod openapi;
mod patch;

#[cfg(feature = "openapi")]
pub use self::openapi::narrow_edit_batch;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    Definition, MAX_DEFINITION_BYTES, MAX_INSTRUCTION_OFFSET, MAX_INSTRUCTIONS_CHARS, document,
    invalid,
};
use crate::Result;

/// Maximum configuration operations in one batch.
pub const MAX_CONFIG_OPS: usize = 64;
/// Maximum instruction ranges in one batch.
pub const MAX_INSTRUCTION_EDITS: usize = 32;
const MAX_POINTER_CHARS: usize = 512;

/// One JSON Patch operation on the setup: `add`, `replace` or `remove`.
//
// copy/move/test are left out: they add no editing power and widen what a
// model proposal could express. The published schema is `openapi::OpShape`,
// which carries the bounds a derive here could not.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
pub enum ConfigOp {
    /// Insert a member or array element below config.
    Add {
        /// JSON Pointer into mutable setup fields.
        #[garde(length(chars, min = 1, max = MAX_POINTER_CHARS))]
        path: String,
        /// The new value. The whole edited setup is validated before it is kept.
        #[garde(skip)]
        value: Value,
    },
    /// Replace an existing member or array element below config, or the description.
    Replace {
        /// JSON Pointer into mutable setup fields.
        #[garde(length(chars, min = 1, max = MAX_POINTER_CHARS))]
        path: String,
        /// The new value.
        #[garde(skip)]
        value: Value,
    },
    /// Remove an existing optional member or array element below config.
    Remove {
        /// JSON Pointer into mutable setup fields.
        #[garde(length(chars, min = 1, max = MAX_POINTER_CHARS))]
        path: String,
    },
}

/// A UTF-8 byte range against the exact base instructions.
///
/// Offsets are bytes, the unit the stored text is sliced in; the replacement
/// text is bounded in characters, like every other Definition text.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct InstructionEdit {
    /// Inclusive start in the unmodified base.
    #[garde(range(max = MAX_INSTRUCTION_OFFSET))]
    pub start_byte: usize,
    /// Exclusive end in the unmodified base.
    #[garde(range(max = MAX_INSTRUCTION_OFFSET))]
    pub end_byte: usize,
    /// Replacement text.
    #[garde(length(chars, max = MAX_INSTRUCTIONS_CHARS), custom(afd_validate::rules::nul_free))]
    pub text: String,
}

/// Validated as one bounded batch by both drawer and model proposal adapters.
#[derive(Debug, Clone, Default, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct EditBatch {
    /// Configuration and description operations, applied in their declared order.
    #[serde(default)]
    #[garde(length(max = MAX_CONFIG_OPS), dive)]
    pub config_ops: Vec<ConfigOp>,
    /// Non-overlapping ranges, always interpreted against the base instructions.
    #[serde(default)]
    #[garde(length(max = MAX_INSTRUCTION_EDITS), dive)]
    pub instruction_edits: Vec<InstructionEdit>,
}

impl Definition {
    /// Atomically applies a batch to a copy and validates the entire resulting setup.
    ///
    /// # Errors
    /// Refuses unsupported paths, overlap, invalid UTF-8 ranges and invalid final policy.
    pub fn edit(&self, batch: EditBatch) -> Result<Self> {
        document::bounded(&serde_json::to_string(&batch)?, MAX_DEFINITION_BYTES)?;
        let batch = garde::Unvalidated::new(batch).validate()?;
        replacement_bound(&batch)?;
        if batch.config_ops.is_empty() && batch.instruction_edits.is_empty() {
            return Err(invalid("an edit must contain at least one operation"));
        }
        let mut value = serde_json::to_value(self)?;
        patch::apply(&mut value, &batch)?;
        *value
            .get_mut("instructions_md")
            .ok_or_else(|| invalid("instructions member is absent"))? =
            Value::String(instructions(self.instructions_md(), &batch)?);
        Self::from_value(value)
    }
}

/// Every character one batch writes, across config values and instruction text.
#[derive(garde::Validate)]
struct ReplacementChars {
    #[garde(range(max = MAX_INSTRUCTIONS_CHARS))]
    chars: usize,
}

fn replacement_bound(batch: &garde::Valid<EditBatch>) -> Result<()> {
    let values = batch.config_ops.iter().map(|operation| match operation {
        ConfigOp::Add { value, .. } | ConfigOp::Replace { value, .. } => text_chars(value),
        ConfigOp::Remove { .. } => 0,
    });
    let instructions = batch
        .instruction_edits
        .iter()
        .map(|edit| edit.text.chars().count());
    let chars = values
        .chain(instructions)
        .fold(0_usize, usize::saturating_add);
    garde::Unvalidated::new(ReplacementChars { chars }).validate()?;
    Ok(())
}

fn text_chars(value: &Value) -> usize {
    match value {
        Value::String(text) => text.chars().count(),
        Value::Array(values) => values.iter().map(text_chars).fold(0, usize::saturating_add),
        Value::Object(values) => values
            .values()
            .map(text_chars)
            .fold(0, usize::saturating_add),
        _scalar => 0,
    }
}

fn instructions(base: &str, batch: &garde::Valid<EditBatch>) -> Result<String> {
    let mut ranges: Vec<_> = batch.instruction_edits.iter().collect();
    ranges.sort_unstable_by_key(|range| (range.start_byte, range.end_byte));
    let mut end = 0;
    let mut output = String::new();
    let mut previous = None;
    for range in ranges {
        if range.start_byte < end
            || range.start_byte > range.end_byte
            || !base.is_char_boundary(range.start_byte)
            || !base.is_char_boundary(range.end_byte)
            || previous == Some(range.start_byte)
        {
            return Err(invalid(
                "instruction ranges must be valid non-overlapping UTF-8 boundaries",
            ));
        }
        output.push_str(&base[end..range.start_byte]);
        output.push_str(&range.text);
        end = range.end_byte;
        previous = Some(range.start_byte);
    }
    output.push_str(&base[end..]);
    document::bounded_chars(&output, MAX_INSTRUCTIONS_CHARS)?;
    Ok(output)
}
