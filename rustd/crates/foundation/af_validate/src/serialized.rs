//! Measure serialized bytes without retaining a rejected representation.

use std::io::{self, Write};

type Result<T, E = serde_json::Error> = core::result::Result<T, E>;

/// Exact encoded JSON bytes without allocating an encoded copy.
///
/// # Errors
/// Preserves the serializer's original failure.
pub fn serialized_len<T: serde::Serialize + ?Sized>(value: &T) -> Result<usize> {
    measure(value, usize::MAX)
}

/// Whether a complete value serializes within its declared byte ceiling.
/// Serialization stops at the first write that would exceed the bound.
#[must_use]
pub fn serialized_within<T: serde::Serialize + ?Sized>(value: &T, max_bytes: usize) -> bool {
    measure(value, max_bytes).is_ok()
}

fn measure<T: serde::Serialize + ?Sized>(value: &T, max_bytes: usize) -> Result<usize> {
    let mut counter = Counter {
        remaining: max_bytes,
    };
    serde_json::to_writer(&mut counter, value)?;
    Ok(max_bytes - counter.remaining)
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
    use super::{Counter, Result, serialized_len, serialized_within};
    use std::io::Write as _;

    #[test]
    fn flushing_retains_the_measured_bytes() -> Result<(), std::io::Error> {
        let mut counter = Counter { remaining: 10 };
        counter.write_all(b"abc")?;
        counter.flush()?;
        assert_eq!(counter.remaining, 7);
        Ok(())
    }

    #[test]
    fn complete_json_bounds_count_utf8_escapes_keys_and_collection_syntax() {
        for (value, bytes) in [
            (serde_json::json!({"k":"a"}), 9),
            (serde_json::json!({"k":"界\n"}), 13),
            (serde_json::json!([true, null]), 11),
        ] {
            assert_eq!(serialized_len(&value).ok(), Some(bytes));
            assert!(serialized_within(&value, bytes));
            assert!(!serialized_within(&value, bytes - 1));
            assert!(!serialized_within(&value, 0));
        }
    }

    #[test]
    fn exact_measurements_preserve_a_serializer_failure() {
        struct Refused;

        impl serde::Serialize for Refused {
            fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("fixture serialization refused"))
            }
        }

        assert!(serialized_len(&Refused).is_err_and(|error| {
            error.is_data() && error.to_string() == "fixture serialization refused"
        }));
        assert!(!serialized_within(&Refused, usize::MAX));
    }
}
