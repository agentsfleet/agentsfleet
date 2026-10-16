//! Development isolation without changing the logical application key names.

use crate::error::{Error, ErrorKind, Result};

/// Optional development namespace. Empty configuration preserves deployed keys.
pub const NAMESPACE_KNOB: &str = "AGENTSFLEET_DEV_NAMESPACE";
const PREFIX: &str = "dev:";
const MAX_NAMESPACE_BYTES: usize = 64;

#[cfg(test)]
mod tests;

#[derive(Debug, garde::Validate)]
struct Input<'a> {
    #[garde(length(bytes, max = MAX_NAMESPACE_BYTES), custom(afd_validate::charset(is_namespace_char)))]
    value: &'a str,
}

fn is_namespace_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '-' | '_')
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Namespace(String);

impl Namespace {
    pub(crate) fn parse(raw: &str) -> Result<Self> {
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        let input = garde::Unvalidated::new(Input { value: raw })
            .validate()
            .map_err(|source| Error::new(ErrorKind::InvalidNamespace { source }))?;
        Ok(Self::from_valid(&input))
    }

    fn from_valid(input: &garde::Valid<Input<'_>>) -> Self {
        Self(format!("{PREFIX}{}:", input.value))
    }

    pub(crate) fn key(&self, logical: &str) -> String {
        format!("{}{logical}", self.0)
    }

    pub(crate) fn logical<'a>(&self, physical: &'a str) -> Option<&'a str> {
        physical.strip_prefix(&self.0)
    }
}
