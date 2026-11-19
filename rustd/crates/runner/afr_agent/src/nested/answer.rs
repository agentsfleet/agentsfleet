//! A nested tool's answer as the model reads it: the shape `guffbolt` gives
//! it, encoded as JSON.

use std::fmt::Debug;

use af_core::error_code;
use guffbolt::ToolOutput;
use serde::Serialize;

pub(super) const EVENT_ANSWER_ENCODE_FAILED: &str = "nested_answer_encode_failed";

/// `value` as a succeeded call's text. These shapes are numbers, names and
/// strings, which JSON always takes; were one refused, the call still did
/// what it says (a spawned child runs on), so it stays succeeded and the
/// model reads the value's debug form rather than nothing.
pub(super) fn json<T: Serialize + Debug>(value: &T) -> ToolOutput {
    let text = serde_json::to_string(value).unwrap_or_else(|refused| unencoded(value, &refused));
    ToolOutput::succeeded(text)
}

/// `value` in its debug form, the refusal logged under the registry code
/// this crate's internal failures carry.
fn unencoded(value: &impl Debug, refused: &serde_json::Error) -> String {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = refused.to_string();
    let event = EVENT_ANSWER_ENCODE_FAILED;
    tracing::warn!(error_code = code, reason, event);
    format!("{value:?}")
}
