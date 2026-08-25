//! What may wake a fleet, and how a signed delivery proves itself.

use garde::Validate;
use serde::Deserialize;

use super::predicate::{is_repository, is_token};
use super::{
    MAX_CREDENTIAL_LEN, MAX_EVENT_LEN, MAX_EVENTS, MAX_REFERENCE_LEN, MAX_REPOSITORIES,
    MAX_REPOSITORY_LEN, MAX_SIGNATURE_HEADER_LEN,
};

/// One entry of `triggers`.
///
/// An internally-tagged enum, so `type` selects the variant and each variant
/// carries only its own keys — the Zig's `union(FleetTriggerType)` expressed
/// where the compiler can check it. An unrecognised `type` becomes a serde
/// error that NAMES the accepted variants, which is strictly more than the
/// Zig's opaque `InvalidTriggerType`.
#[derive(Debug, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Trigger {
    /// Woken by a signed delivery from an external provider.
    Webhook {
        /// Which provider.
        #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
        source: Option<String>,
        /// The event-name allow-list.
        ///
        /// Absent means every event. A list that is PRESENT and names nothing
        /// would subscribe to nothing, so the minimum is one.
        #[garde(inner(
            length(min = 1, max = MAX_EVENTS),
            inner(length(chars, min = 1, max = MAX_EVENT_LEN), custom(is_token))
        ))]
        events: Option<Vec<String>>,
        /// The App-ingress repository binding.
        #[garde(inner(
            length(min = 1, max = MAX_REPOSITORIES),
            inner(length(chars, min = 1, max = MAX_REPOSITORY_LEN), custom(is_repository))
        ))]
        repositories: Option<Vec<String>>,
        /// A vault-key override, so two fleets on one source can hold
        /// different secrets.
        #[garde(inner(length(chars, min = 1, max = MAX_CREDENTIAL_LEN)))]
        credential_name: Option<String>,
        /// How that delivery proves itself.
        #[garde(dive)]
        signature: Option<Signature>,
    },
    /// Woken on a schedule.
    Cron {
        /// The schedule expression.
        #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
        schedule: Option<String>,
        /// Which zone the schedule is read in.
        #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
        timezone: Option<String>,
        /// What the scheduled run is told it is for.
        #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
        message: Option<String>,
    },
    /// Woken by an authenticated API call, which carries no further config.
    Api,
}

/// A webhook trigger's signature block.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Signature {
    /// The vault key holding the shared secret.
    #[garde(inner(length(chars, min = 1, max = MAX_CREDENTIAL_LEN)))]
    pub(crate) secret_ref: Option<String>,
    /// The header the signature arrives in, when overriding the provider's.
    #[garde(inner(length(chars, min = 1, max = MAX_SIGNATURE_HEADER_LEN)))]
    pub(crate) header: Option<String>,
    /// The prefix that header's value carries, when overriding.
    #[garde(inner(length(chars, max = MAX_SIGNATURE_HEADER_LEN)))]
    pub(crate) prefix: Option<String>,
    /// The signed-timestamp header, when overriding.
    #[garde(inner(length(chars, min = 1, max = MAX_SIGNATURE_HEADER_LEN)))]
    pub(crate) ts_header: Option<String>,
}
