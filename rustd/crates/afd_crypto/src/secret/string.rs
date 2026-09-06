//! Shared ownership for secret text: explicit exposure and redacted diagnostics.
use crate::error::Result;
use serde::{Deserialize, Deserializer};
use std::fmt;
use zeroize::Zeroizing;

/// Secret text whose owned buffer is wiped on drop, including unwinding.
/// No equality, display, serialization, or implicit dereferencing is exposed.
#[derive(Clone)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Moves an existing buffer into its wiping guard.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// Borrows the secret for a call that needs its plaintext.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty, without exposing it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(redacted)")
    }
}
