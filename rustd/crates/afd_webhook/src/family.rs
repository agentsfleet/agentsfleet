//! Declarable signing recipes. Configuration chooses a recipe, never signed bytes.

use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::vendor::svix::{self, SvixHeaders, SvixSecret};
use crate::{Refusal, Scheme, Verdict};

/// Stable configuration tokens, shared by prompts and documentation.
pub const BODY_HEX_SHA256_TOKEN: &str = "body-hex-sha256";
/// Bare hex body recipe.
pub const BODY_HEX_TOKEN: &str = "body-hex";
/// Base64 body recipe.
pub const BODY_BASE64_SHA256_TOKEN: &str = "body-base64-sha256";
/// Slack timestamp recipe.
pub const SLACK_V0_TOKEN: &str = "slack-v0";
/// Svix identifier and timestamp recipe.
pub const SVIX_V1_TOKEN: &str = "svix-v1";

/// One complete verification recipe, including its encoding and signed fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Raw body, hex digest prefixed with `sha256=`.
    BodyHexSha256,
    /// Raw body, bare hex digest.
    BodyHex,
    /// Raw body, standard padded base64 digest.
    BodyBase64Sha256,
    /// Slack's version and timestamp bound to the body.
    SlackV0,
    /// Svix's identifier and timestamp bound to the body.
    SvixV1,
}

impl Family {
    /// Every recipe accepted in a declaration.
    pub const ALL: &[Self] = &[
        Self::BodyHexSha256,
        Self::BodyHex,
        Self::BodyBase64Sha256,
        Self::SlackV0,
        Self::SvixV1,
    ];

    /// The stable token used by configuration, prompts and documentation.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::BodyHexSha256 => BODY_HEX_SHA256_TOKEN,
            Self::BodyHex => BODY_HEX_TOKEN,
            Self::BodyBase64Sha256 => BODY_BASE64_SHA256_TOKEN,
            Self::SlackV0 => SLACK_V0_TOKEN,
            Self::SvixV1 => SVIX_V1_TOKEN,
        }
    }

    /// A registry source's implied recipe. Custom sources declare one instead.
    #[must_use]
    pub fn for_source(source: &str) -> Option<Self> {
        Scheme::for_source(source).map(|scheme| match scheme {
            Scheme::BodyHex => Self::BodyHexSha256,
            Scheme::BodyHexBare => Self::BodyHex,
            Scheme::SlackV0 => Self::SlackV0,
        })
    }

    /// Checks raw bytes under this recipe. Headers cannot select the recipe.
    pub fn verify_at(self, secret: &SecretBytes, proof: Proof<'_>, now: i64) -> Verdict {
        match self {
            Self::BodyHexSha256 => proof.verify(Scheme::BodyHex, secret, now),
            Self::BodyHex => proof.verify(Scheme::BodyHexBare, secret, now),
            Self::SlackV0 => proof.verify(Scheme::SlackV0, secret, now),
            Self::BodyBase64Sha256 => verify_base64(secret, proof),
            Self::SvixV1 => verify_svix(secret, proof, now),
        }
    }
}

/// The headers and untouched body presented to one already-selected recipe.
#[derive(Debug, Clone, Copy)]
pub struct Proof<'a> {
    /// The signature header, selected by the stored declaration.
    pub signature: Option<&'a str>,
    /// The fixed timestamp header, where the recipe binds one.
    pub timestamp: Option<&'a str>,
    /// The fixed identifier header, where the recipe binds one.
    pub id: Option<&'a str>,
    /// Bytes before any body parsing.
    pub body: &'a [u8],
}

impl Proof<'_> {
    fn verify(self, scheme: Scheme, secret: &SecretBytes, now: i64) -> Verdict {
        scheme.verify_at(secret, self.signature, self.timestamp, self.body, now)
    }
}

fn verify_base64(secret: &SecretBytes, proof: Proof<'_>) -> Verdict {
    if secret.is_empty() {
        return Verdict::Refused(Refusal::Unconfigured);
    }
    let offered = proof
        .signature
        .and_then(|text| STANDARD.decode(text).ok())
        .and_then(|bytes| HmacSha256Tag::from_slice(&bytes).ok());
    let expected = HmacSha256Tag::compute_peppered(secret, &[proof.body]);
    match offered {
        Some(tag) if expected.verify(&tag).is_ok() => Verdict::Verified,
        _ => Verdict::Refused(Refusal::Signature),
    }
}

fn verify_svix(secret: &SecretBytes, proof: Proof<'_>, now: i64) -> Verdict {
    let Some(secret) = std::str::from_utf8(secret.expose())
        .ok()
        .and_then(SvixSecret::parse)
    else {
        return Verdict::Refused(Refusal::Unconfigured);
    };
    svix::verify_at(
        &secret,
        SvixHeaders {
            id: proof.id.unwrap_or_default(),
            timestamp: proof.timestamp.unwrap_or_default(),
            signature: proof.signature.unwrap_or_default(),
        },
        proof.body,
        now,
    )
}
