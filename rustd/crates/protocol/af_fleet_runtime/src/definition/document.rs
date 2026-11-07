//! A retained JSON setup backed by the shared typed runtime parser.

use serde::Serialize;
use serde_json::Value;

use super::{invalid, normalize_text};
use crate::{FleetConfig, Result};

/// Configuration bytes and the policy they were validated into.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ConfigDocument {
    value: Value,
    #[serde(skip)]
    runtime: FleetConfig,
}

/// The runtime block's key in a configuration document.
const RUNTIME_BLOCK: &str = "x-agentsfleet";
/// The budget's key inside the runtime block.
const BUDGET: &str = "budget";
/// The trigger set's key inside the runtime block.
const TRIGGERS: &str = "triggers";
/// Why a configuration with no runtime block or no budget was refused.
const REASON_NO_BUDGET: &str = "budget is required";

impl ConfigDocument {
    pub(super) fn parse(mut value: Value) -> Result<Self> {
        normalize(&mut value);
        let runtime = FleetConfig::authored(&serde_json::to_string(&value)?)?;
        let block = value
            .get_mut(RUNTIME_BLOCK)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| invalid(REASON_NO_BUDGET))?;
        let budget = block
            .get_mut(BUDGET)
            .ok_or_else(|| invalid(REASON_NO_BUDGET))?;
        *budget = serde_json::to_value(runtime.budget())?;
        // An absent trigger set and an empty one are the same fleet, so they
        // are stored as one document and hash to one digest.
        block
            .entry(TRIGGERS)
            .or_insert_with(|| Value::Array(Vec::new()));
        Ok(Self { value, runtime })
    }

    /// The normalized supported JSON document, including optional policy fields.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// The policy proved by the common fleet parser.
    #[must_use]
    pub const fn runtime(&self) -> &FleetConfig {
        &self.runtime
    }
}

#[derive(garde::Validate)]
#[garde(context(usize as maximum))]
struct Bounded<'a> {
    #[garde(length(bytes, max = *maximum))]
    text: &'a str,
}

#[derive(garde::Validate)]
#[garde(context(usize as maximum))]
struct BoundedChars<'a> {
    #[garde(length(chars, max = *maximum))]
    text: &'a str,
}

/// Refuses `text` over `maximum` bytes: the envelope read before parsing.
pub(super) fn bounded(text: &str, maximum: usize) -> Result<()> {
    garde::Unvalidated::new(Bounded { text }).validate_with(&maximum)?;
    Ok(())
}

/// Refuses `text` over `maximum` characters: a Definition text's own bound.
pub(super) fn bounded_chars(text: &str, maximum: usize) -> Result<()> {
    garde::Unvalidated::new(BoundedChars { text }).validate_with(&maximum)?;
    Ok(())
}

/// Normalize text without dropping policy keys or reordering arrays.
pub(super) fn normalize(value: &mut Value) {
    match value {
        Value::Object(object) => object.values_mut().for_each(normalize),
        Value::Array(array) => array.iter_mut().for_each(normalize),
        Value::String(text) => *text = normalize_text(text),
        _scalar => {}
    }
}
