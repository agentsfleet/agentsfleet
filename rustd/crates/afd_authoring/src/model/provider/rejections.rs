//! A rejected request never generated tokens; ambiguous failures remain unknown.

use crate::{Result, error::ErrorKind};
use afd_crypto::secret::SecretString;
use serde_json::Value;

const ERROR: &str = "error";

/// Parse before committing intent, using the same HTTP header parser as dispatch.
pub(in crate::model) fn validate_key(key: &SecretString) -> Result<()> {
    http::HeaderValue::from_bytes(key.expose().as_bytes())
        .map(|_| ())
        .map_err(|_invalid| ErrorKind::Model.into())
}

/// Only a standalone HTTP authentication error proves rejection. Any response
/// with output, usage, or an unrecognized envelope keeps normal receipt decoding.
pub(super) fn authentication(status: http::StatusCode, document: &Value) -> bool {
    status == http::StatusCode::UNAUTHORIZED
        && document.as_object().is_some_and(|object| {
            object.len() == 1 && object.get(ERROR).is_some_and(Value::is_object)
        })
}
