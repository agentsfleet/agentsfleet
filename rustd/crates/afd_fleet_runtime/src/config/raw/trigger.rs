//! What may wake a fleet, and how a signed delivery proves itself.

use afd_validate::charset;
use garde::Validate;
use serde::Deserialize;

use super::predicate::{is_not_reserved_source, is_repository, is_token};
use super::{
    MAX_CHANNEL_ID_LEN, MAX_CREDENTIAL_LEN, MAX_EVENT_LEN, MAX_EVENTS, MAX_REFERENCE_LEN,
    MAX_REPOSITORIES, MAX_REPOSITORY_LEN, MENTION_CHANNELS,
};
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
        signature: Option<Signature>,
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

/// A Svix-signed webhook: the secret its deliveries are verified with, at
/// `/v1/webhooks/svix/{fleet_id}`.
// Closed: the header, prefix and timestamp keys this once carried were never
// read by any route, so a document still naming them is told so.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionWebhookSignature))]
pub(crate) struct Signature {
    /// The secret name that holds the Svix signing secret.
    #[garde(inner(length(chars, min = 1, max = MAX_CREDENTIAL_LEN)))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) secret_ref: Option<String>,
}
