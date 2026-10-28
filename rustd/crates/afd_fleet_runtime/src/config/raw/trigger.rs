//! What may wake a fleet, and how a signed delivery proves itself.

use afd_validate::charset;
use garde::Validate;
use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::predicate::{is_not_reserved_source, is_repository, is_token};
use super::{
    MAX_CHANNEL_ID_LEN, MAX_CREDENTIAL_LEN, MAX_EVENT_LEN, MAX_EVENTS, MAX_REFERENCE_LEN,
    MAX_REPOSITORIES, MAX_REPOSITORY_LEN, MENTION_CHANNELS,
};
use crate::error::Result;
use crate::name::is_slug_char;

/// One way a fleet can start a run. `type` selects the kind.
//
// An internally-tagged enum, so `type` selects the variant and each variant
// carries only its own keys, where the compiler can check them. An
// unrecognised `type` becomes a serde error that NAMES the accepted variants.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionTrigger))]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Trigger {
    /// Starts on a signed delivery from an external provider.
    Webhook {
        /// The provider that sends the delivery, as a lowercase slug: it is the
        /// last segment of the URL the fleet's deliveries are sent to.
        #[garde(inner(
            length(chars, min = 1, max = MAX_REFERENCE_LEN),
            custom(charset(is_slug_char)),
            custom(is_not_reserved_source)
        ))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        source: Option<String>,
        /// The event names that start a run. Omit it to accept every event.
        // A list that is PRESENT and names nothing would subscribe to nothing,
        // so the minimum is one.
        #[garde(inner(
            length(min = 1, max = MAX_EVENTS),
            inner(length(chars, min = 1, max = MAX_EVENT_LEN), custom(is_token))
        ))]
        events: Option<Vec<String>>,
        /// The repositories whose deliveries may start a run, as `owner/name`.
        #[garde(inner(
            length(min = 1, max = MAX_REPOSITORIES),
            inner(length(chars, min = 1, max = MAX_REPOSITORY_LEN), custom(is_repository))
        ))]
        repositories: Option<Vec<String>>,
        /// A secret name to use instead of the provider's default, so two fleets
        /// on one provider can hold different secrets.
        #[garde(inner(length(chars, min = 1, max = MAX_CREDENTIAL_LEN)))]
        credential_name: Option<String>,
        /// How the delivery proves it came from the provider.
        #[garde(dive)]
        verifier: Option<Verifier>,
    },
    /// Starts on a schedule.
    Cron {
        /// The schedule, as a cron expression.
        #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        schedule: Option<String>,
        /// The time zone the schedule is read in.
        #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
        timezone: Option<String>,
        /// The message each scheduled run receives.
        #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
        message: Option<String>,
    },
    /// Starts when someone mentions the bot in one chat channel.
    Mention {
        /// The chat provider.
        #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        source: Option<String>,
        /// The channel identifier, as a list with exactly one entry.
        // A list so the key reads like `repositories`; exactly one entry, so a
        // fleet speaks to one audience. The identifier's shape is checked where
        // it is typed.
        #[garde(inner(
            length(min = MENTION_CHANNELS, max = MENTION_CHANNELS),
            inner(length(chars, min = 1, max = MAX_CHANNEL_ID_LEN))
        ))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        channels: Option<Vec<String>>,
    },
}

/// The complete signing recipe. Fixed-header recipes accept no extra fields.
#[derive(Debug, Deserialize, Validate)]
#[serde(tag = "scheme", deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionWebhookVerifier))]
pub(crate) enum Verifier {
    /// Raw body with a prefixed hex digest.
    #[serde(rename = "body-hex-sha256")]
    BodyHexSha256 {
        /// Header holding the signature.
        #[garde(inner(length(chars, min = 1, max = MAX_HEADER_LEN), custom(charset(is_header_char))))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        header: Option<String>,
    },
    /// Raw body with a bare hex digest.
    #[serde(rename = "body-hex")]
    BodyHex {
        /// Header holding the signature.
        #[garde(inner(length(chars, min = 1, max = MAX_HEADER_LEN), custom(charset(is_header_char))))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        header: Option<String>,
    },
    /// Raw body with a base64 digest.
    #[serde(rename = "body-base64-sha256")]
    BodyBase64Sha256 {
        /// Header holding the signature.
        #[garde(inner(length(chars, min = 1, max = MAX_HEADER_LEN), custom(charset(is_header_char))))]
        #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
        header: Option<String>,
    },
    /// Fixed Slack headers with a signed timestamp.
    #[serde(rename = "slack-v0", deserialize_with = "fixed_headers")]
    SlackV0,
    /// Fixed Svix headers with a signed identifier and timestamp.
    #[serde(rename = "svix-v1", deserialize_with = "fixed_headers")]
    SvixV1,
}

// Serde's unit-variant reader ignores remaining keys, so fixed recipes read an
// empty map explicitly. The enum tag has already been consumed at this point.
fn fixed_headers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    deserializer.deserialize_map(FixedHeaders)
}

struct FixedHeaders;

impl<'de> Visitor<'de> for FixedHeaders {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a verifier with no configurable headers")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        match map.next_key::<String>()? {
            Some(key) => Err(A::Error::unknown_field(&key, &[])),
            None => Ok(()),
        }
    }
}

/// Bound on an authored signature header, in ASCII characters.
pub(crate) const MAX_HEADER_LEN: usize = 128;
/// HTTP token grammar; a header name cannot inject whitespace or delimiters.
#[cfg(feature = "openapi")]
pub(crate) const HEADER_PATTERN: &str = "^[!#$%&'*+.^_`|~0-9A-Za-z-]+$";

const fn is_header_char(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}
