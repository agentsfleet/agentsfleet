//! A secret held in memory: never printed, and wiped when it drops.

use std::fmt;

use zeroize::Zeroizing;

/// What every secret prints as.
const REDACTED: &str = "[redacted]";

/// A token this runner holds: the runner's own, or one minted for a lease.
///
/// `Debug` never prints it, and its bytes are overwritten when it drops, so a
/// token does not linger in memory the allocator hands out again.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Holds `value` as a secret.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// The secret, for the one call that presents it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn a_secret_is_never_printed_and_is_presented_whole() {
        let secret = Secret::new("agt_r_value".to_owned());

        assert_eq!(format!("{secret:?}"), "[redacted]");
        assert_eq!(secret.expose(), "agt_r_value");
    }
}
