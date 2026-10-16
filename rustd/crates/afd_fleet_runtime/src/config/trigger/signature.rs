//! A Svix-signed webhook trigger's secret.
//!
//! Split from `trigger/mod.rs` along the one line that file already had:
//! everything here reads a single signature block, and nothing in it knows
//! about the trigger SET.

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

/// Why a signature block was refused.
const REASON_NO_SECRET: &str = "it names no secret";

/// The secret a Svix-signed delivery is verified with.
///
/// Read by the Svix route (`afd_ingress::secret::svix_secret`); a trigger that
/// declares one is verifiable whatever its source is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookSignature {
    /// The vault key holding the Svix signing secret.
    secret_ref: Box<str>,
}

impl WebhookSignature {
    /// The vault key holding the Svix signing secret.
    #[must_use]
    pub fn secret_ref(&self) -> &str {
        &self.secret_ref
    }

    /// Reads an authored block, which must name its secret.
    ///
    /// # Errors
    /// [`ErrorKind::InvalidSignatureConfig`] naming the source and the rule.
    pub(super) fn resolve(authored: raw::Signature, source: &str) -> Result<Self> {
        authored
            .secret_ref
            .filter(|value| !value.is_empty())
            .map(|secret_ref| Self {
                secret_ref: secret_ref.into(),
            })
            .ok_or_else(|| {
                Error::from(ErrorKind::InvalidSignatureConfig {
                    provider: source.into(),
                    reason: REASON_NO_SECRET,
                })
            })
    }
}
