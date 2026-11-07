//! Measure serialized bytes without retaining a rejected representation.

use std::io::{self, Write};

/// Whether a complete value serializes within its declared byte ceiling.
/// Serialization stops at the first write that would exceed the bound.
#[must_use]
pub fn serialized_within<T: serde::Serialize + ?Sized>(value: &T, max_bytes: usize) -> bool {
    serde_json::to_writer(
        Counter {
            remaining: max_bytes,
        },
        value,
    )
    .is_ok()
}

struct Counter {
    remaining: usize,
}

impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.remaining = self
            .remaining
            .checked_sub(buf.len())
            .ok_or(io::ErrorKind::FileTooLarge)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::serialized_within;

    #[test]
    fn complete_json_bounds_count_utf8_escapes_keys_and_collection_syntax() {
        for (value, bytes) in [
            (serde_json::json!({"k":"a"}), 9),
            (serde_json::json!({"k":"界\n"}), 13),
            (serde_json::json!([true, null]), 11),
        ] {
            assert!(serialized_within(&value, bytes));
            assert!(!serialized_within(&value, bytes - 1));
            assert!(!serialized_within(&value, 0));
        }
    }
}
