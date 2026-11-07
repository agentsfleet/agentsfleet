//! Reviewed content identity for native setup: sorted keys and ordered arrays.

use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

use super::{Definition, POLICY_SEMANTICS};
use crate::Result;

const DIGEST_PREFIX: &str = "definition:v1:sha256:";

/// Hashes the normalized Definition and the version of its policy meaning.
///
/// # Errors
/// Propagates JSON serialization failure without dropping its source.
pub fn canonical_digest(definition: &Definition) -> Result<String> {
    let value = json!({
        "definition":definition, "policy_semantics":POLICY_SEMANTICS
    });
    let bytes = serde_json::to_vec(&sorted(value))?;
    Ok(format!(
        "{DIGEST_PREFIX}{}",
        hex::encode(Sha256::digest(bytes))
    ))
}

fn sorted(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        Value::Number(number) => {
            let rendered = number.to_string();
            let integer = rendered
                .strip_suffix(".0")
                .and_then(|text| text.parse::<i64>().ok());
            integer.map_or(Value::Number(number), |value| Value::Number(value.into()))
        }
        scalar => scalar,
    }
}
