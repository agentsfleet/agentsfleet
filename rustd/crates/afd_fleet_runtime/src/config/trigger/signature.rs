//! How a webhook trigger's signed delivery proves itself.
//!
//! Split from `trigger.rs` at the file cap, along the one line that file
//! already had: everything here completes a single signature block from the
//! provider registry, and nothing in it knows about the trigger SET.

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};
use crate::provider::ProviderRegistry;

/// Why a signature block was refused.
const REASON_NO_SECRET: &str = "it names no secret";
/// See [`REASON_NO_SECRET`].
const REASON_NO_HEADER: &str =
    "the provider is not one this daemon knows, so a header must be named";

/// How a signed delivery proves itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookSignature {
    /// The header the signature arrives in.
    header: Box<str>,
    /// What that header's value is prefixed with; empty when it carries the
    /// digest bare.
    prefix: Box<str>,
    /// The header carrying the signed timestamp, for schemes that bind one.
    timestamp_header: Option<Box<str>>,
    /// The vault key holding the shared secret.
    secret_ref: Box<str>,
}

impl WebhookSignature {
    /// The header the signature arrives in.
    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }

    /// What that header's value is prefixed with.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The signed-timestamp header, for schemes that bind one.
    #[must_use]
    pub fn timestamp_header(&self) -> Option<&str> {
        self.timestamp_header.as_deref()
    }

    /// The vault key holding the shared secret.
    #[must_use]
    pub fn secret_ref(&self) -> &str {
        &self.secret_ref
    }

    /// Completes an authored block from what the provider already knows.
    ///
    /// An authored value always wins; the registry only fills what was left
    /// out. A source the registry does not know is not a failure by itself —
    /// it is a failure only when the block also names no header, because then
    /// nothing can say where the signature arrives.
    ///
    /// # Errors
    /// [`Error::InvalidSignatureConfig`] naming the source and the rule.
    pub(super) fn resolve(
        authored: raw::Signature,
        source: &str,
        providers: &dyn ProviderRegistry,
    ) -> Result<Self> {
        let refuse = |reason| {
            Error::from(ErrorKind::InvalidSignatureConfig {
                provider: source.into(),
                reason,
            })
        };

        let secret_ref = authored
            .secret_ref
            .filter(|value| !value.is_empty())
            .ok_or_else(|| refuse(REASON_NO_SECRET))?;

        let known = providers.resolve(source);

        let header = authored
            .header
            .or_else(|| known.map(|scheme| scheme.signature_header().to_owned()))
            .ok_or_else(|| refuse(REASON_NO_HEADER))?;

        Ok(Self {
            header: header.into(),
            // An unknown provider with an authored header carries no prefix
            // rather than borrowing one: a prefix that does not match the
            // scheme would make every signature fail to compare.
            prefix: authored
                .prefix
                .or_else(|| known.map(|scheme| scheme.signature_prefix().to_owned()))
                .unwrap_or_default()
                .into(),
            timestamp_header: authored
                .ts_header
                .or_else(|| known.and_then(|scheme| scheme.timestamp_header().map(str::to_owned)))
                .map(Into::into),
            secret_ref: secret_ref.into(),
        })
    }
}
