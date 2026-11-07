//! A validated recipe: only body-only recipes carry an authored header.

use afd_webhook::Family;

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

const REASON_REGISTRY: &str = "a built-in source must use its registry verifier";
const REASON_NO_HEADER: &str = "a body signing recipe requires a signature header";

/// A complete verifier declaration, bounded and coherent at parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookVerifier {
    /// Prefixed hex tag over the raw body.
    BodyHexSha256(Box<str>),
    /// Bare hex tag over the raw body.
    BodyHex(Box<str>),
    /// Standard base64 tag over the raw body.
    BodyBase64Sha256(Box<str>),
    /// Slack's fixed headers and signed bytes.
    SlackV0,
    /// Svix's fixed headers and signed bytes.
    SvixV1,
}

impl WebhookVerifier {
    /// The recipe this declaration selects, independent of incoming headers.
    #[must_use]
    pub const fn family(&self) -> Family {
        match self {
            Self::BodyHexSha256(_) => Family::BodyHexSha256,
            Self::BodyHex(_) => Family::BodyHex,
            Self::BodyBase64Sha256(_) => Family::BodyBase64Sha256,
            Self::SlackV0 => Family::SlackV0,
            Self::SvixV1 => Family::SvixV1,
        }
    }

    /// An authored header, only for recipes that sign the body alone.
    #[must_use]
    pub fn header(&self) -> Option<&str> {
        match self {
            Self::BodyHexSha256(header)
            | Self::BodyHex(header)
            | Self::BodyBase64Sha256(header) => Some(header),
            Self::SlackV0 | Self::SvixV1 => None,
        }
    }

    pub(super) fn resolve(authored: raw::Verifier, source: &str) -> Result<Self> {
        let refuse = |reason| {
            Error::from(ErrorKind::InvalidSignatureConfig {
                provider: source.into(),
                reason,
            })
        };
        if Family::for_source(source).is_some() {
            return Err(refuse(REASON_REGISTRY));
        }
        let header = |named: Option<String>| {
            named
                .map(|text| text.to_ascii_lowercase().into_boxed_str())
                .ok_or_else(|| refuse(REASON_NO_HEADER))
        };
        match authored {
            raw::Verifier::BodyHexSha256 { header: name } => header(name).map(Self::BodyHexSha256),
            raw::Verifier::BodyHex { header: name } => header(name).map(Self::BodyHex),
            raw::Verifier::BodyBase64Sha256 { header: name } => {
                header(name).map(Self::BodyBase64Sha256)
            }
            raw::Verifier::SlackV0 => Ok(Self::SlackV0),
            raw::Verifier::SvixV1 => Ok(Self::SvixV1),
        }
    }
}
