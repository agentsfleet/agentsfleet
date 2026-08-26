//! What may wake a fleet.
//!
//! # Why the set is validated, not just each entry
//!
//! Two webhook triggers on one source, or two cron entries, are each
//! individually well-formed and jointly meaningless: the fleet would be woken
//! twice for one delivery, or run twice on one schedule. The Zig checks this
//! with a nested loop inside the parse loop, comparing `activeTag` and then
//! reaching into `existing.webhook.source` — a reach the compiler permits only
//! because the tag was checked one line earlier. Here the check runs over
//! already-typed values, so the comparison is a `match` the compiler proves
//! total.

use crate::config::raw;
use crate::error::{Error, Result};
use crate::provider::ProviderRegistry;

/// Where a cron trigger's schedule is read, when it names no zone.
const DEFAULT_CRON_TIMEZONE: &str = "UTC";
/// What a scheduled run is told it is for, when it says nothing.
const DEFAULT_CRON_MESSAGE: &str = "Scheduled Fleet run";

/// Most triggers one fleet may declare.
const MAX_TRIGGERS: usize = 8;

/// Why a trigger set was refused.
const REASON_SET_EMPTY: &str = "a fleet with no trigger can never be woken";
/// See [`REASON_SET_EMPTY`].
const REASON_SET_TOO_LARGE: &str = "it holds more triggers than the limit";
/// See [`REASON_SET_EMPTY`].
const REASON_DUPLICATE_SOURCE: &str = "two webhook triggers share one source";
/// See [`REASON_SET_EMPTY`].
const REASON_DUPLICATE_CRON: &str = "a fleet may hold only one cron trigger";
/// See [`REASON_SET_EMPTY`].
const REASON_DUPLICATE_API: &str = "a fleet may hold only one api trigger";

/// Why a signature block was refused.
const REASON_NO_SECRET: &str = "it names no secret";
/// See [`REASON_NO_SECRET`].
const REASON_NO_HEADER: &str =
    "the provider is not one this daemon knows, so a header must be named";

/// Why a webhook trigger was refused.
const REASON_NO_SOURCE: &str = "a webhook trigger names no source";
/// Why a cron trigger was refused.
const REASON_NO_SCHEDULE: &str = "a cron trigger names no schedule";

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
    fn resolve(
        authored: raw::Signature,
        source: &str,
        providers: &dyn ProviderRegistry,
    ) -> Result<Self> {
        let refuse = |reason| Error::InvalidSignatureConfig {
            provider: source.into(),
            reason,
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

/// A fleet woken by a signed delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Webhook {
    /// Which provider sends it.
    pub source: Box<str>,
    /// The event allow-list; `None` fires on every event.
    pub events: Option<Box<[Box<str>]>>,
    /// The App-ingress repository binding.
    pub repositories: Option<Box<[Box<str>]>>,
    /// A vault-key override, so two fleets on one source can hold different
    /// secrets.
    pub credential_name: Option<Box<str>>,
    /// How the delivery proves itself.
    pub signature: Option<WebhookSignature>,
}

/// A fleet woken on a schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    /// The schedule expression.
    pub schedule: Box<str>,
    /// Which zone it is read in.
    pub timezone: Box<str>,
    /// What the scheduled run is told it is for.
    pub message: Box<str>,
}

/// What may wake a fleet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// A signed delivery from an external provider.
    Webhook(Webhook),
    /// A schedule.
    Cron(Cron),
    /// An authenticated API call.
    Api,
}

impl Trigger {
    /// Builds one trigger, completing its signature from `providers`.
    ///
    /// # Errors
    /// Whichever rule the trigger broke.
    fn parse(authored: raw::Trigger, providers: &dyn ProviderRegistry) -> Result<Self> {
        match authored {
            raw::Trigger::Webhook {
                source,
                events,
                repositories,
                credential_name,
                signature,
            } => {
                let source =
                    source
                        .filter(|value| !value.is_empty())
                        .ok_or(Error::InvalidTriggerSet {
                            reason: REASON_NO_SOURCE,
                        })?;

                Ok(Self::Webhook(Webhook {
                    // Both lists were bounded by the schema, so what is left
                    // is ownership.
                    events: events.map(own),
                    repositories: repositories.map(own),
                    signature: signature
                        .map(|block| WebhookSignature::resolve(block, &source, providers))
                        .transpose()?,
                    credential_name: credential_name.map(Into::into),
                    source: source.into(),
                }))
            }
            raw::Trigger::Cron {
                schedule,
                timezone,
                message,
            } => Ok(Self::Cron(Cron {
                schedule: schedule
                    .filter(|value| !value.is_empty())
                    .ok_or(Error::InvalidTriggerSet {
                        reason: REASON_NO_SCHEDULE,
                    })?
                    .into(),
                timezone: timezone
                    .unwrap_or_else(|| DEFAULT_CRON_TIMEZONE.to_owned())
                    .into(),
                message: message
                    .unwrap_or_else(|| DEFAULT_CRON_MESSAGE.to_owned())
                    .into(),
            })),
            raw::Trigger::Api => Ok(Self::Api),
        }
    }

    /// The source this trigger answers to, for the uniqueness check.
    fn source(&self) -> Option<&str> {
        match self {
            Self::Webhook(hook) => Some(&hook.source),
            Self::Cron(_) | Self::Api => None,
        }
    }
}

/// Takes ownership of an already-bounded list.
fn own(items: Vec<String>) -> Box<[Box<str>]> {
    items.into_iter().map(Into::into).collect()
}

/// Builds the whole trigger set and proves it is coherent.
///
/// # Errors
/// [`Error::InvalidTriggerSet`] for an arity or uniqueness failure, or
/// whichever rule an individual trigger broke.
pub(crate) fn parse_set(
    authored: Vec<raw::Trigger>,
    providers: &dyn ProviderRegistry,
) -> Result<Box<[Trigger]>> {
    let refuse = |reason| Error::InvalidTriggerSet { reason };

    match authored.len() {
        0 => return Err(refuse(REASON_SET_EMPTY)),
        len if len > MAX_TRIGGERS => return Err(refuse(REASON_SET_TOO_LARGE)),
        _ => {}
    }

    let triggers = authored
        .into_iter()
        .map(|entry| Trigger::parse(entry, providers))
        .collect::<Result<Box<[Trigger]>>>()?;

    prove_unique(&triggers).map(|()| triggers)
}

/// Proves no two triggers would fire for the same thing.
fn prove_unique(triggers: &[Trigger]) -> Result<()> {
    let refuse = |reason| Error::InvalidTriggerSet { reason };

    triggers
        .iter()
        .enumerate()
        .try_fold((), |(), (index, trigger)| {
            let clashes = triggers.iter().skip(index + 1).any(|later| {
                match (trigger, later) {
                    (Trigger::Cron(_), Trigger::Cron(_)) | (Trigger::Api, Trigger::Api) => true,
                    // Two webhooks clash only on one source. Different sources
                    // are the whole point of declaring more than one.
                    (Trigger::Webhook(_), Trigger::Webhook(_)) => {
                        trigger.source() == later.source()
                    }
                    _ => false,
                }
            });

            if !clashes {
                return Ok(());
            }

            Err(refuse(match trigger {
                Trigger::Webhook(_) => REASON_DUPLICATE_SOURCE,
                Trigger::Cron(_) => REASON_DUPLICATE_CRON,
                Trigger::Api => REASON_DUPLICATE_API,
            }))
        })
}
