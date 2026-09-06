//! Guard the owned strings in a parsed secret object and its canonical output.
//! Serde owns parsing; its temporary/error buffers are outside this guard.
use super::SecretBytes;
use crate::error::Result;
use serde_json::{Map, Value};
use zeroize::Zeroize;

/// A parsed secret object whose keys and string values are wiped on drop.
/// The borrowed source and Serde's internal scratch storage have separate owners.
pub struct SecretObject(Map<String, Value>);

impl SecretObject {
    /// Parses an object using Serde's map ordering and value types.
    ///
    /// # Errors
    /// Returns Serde's error for malformed JSON or a non-object.
    pub fn parse(raw: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(raw).map(Self)
    }

    /// Borrows fields for validation or a non-secret metadata projection.
    #[must_use]
    pub const fn fields(&self) -> &Map<String, Value> {
        &self.0
    }

    /// Replaces a string field and wipes the displaced value before releasing it.
    pub fn replace_string(&mut self, field: &str, replacement: &str) {
        if let Some(value) = self.0.get_mut(field) {
            wipe(value);
            *value = Value::String(replacement.to_owned());
        } else {
            self.0
                .insert(field.to_owned(), Value::String(replacement.to_owned()));
        }
    }

    /// Guards the completed canonical output from Serde's serializer.
    /// Serializer scratch storage and reallocations have separate lifetimes.
    ///
    /// # Errors
    /// Returns the serializer's error if the object cannot be encoded.
    pub fn canonical(&self) -> Result<SecretBytes, serde_json::Error> {
        serde_json::to_vec(&self.0).map(SecretBytes::new)
    }
}

impl std::fmt::Debug for SecretObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretObject(redacted)")
    }
}

impl Drop for SecretObject {
    fn drop(&mut self) {
        wipe_fields(&mut self.0);
    }
}

fn wipe_fields(fields: &mut Map<String, Value>) {
    for (mut key, mut value) in std::mem::take(fields) {
        key.zeroize();
        wipe(&mut value);
    }
}

fn wipe(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(wipe),
        Value::Object(fields) => wipe_fields(fields),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
