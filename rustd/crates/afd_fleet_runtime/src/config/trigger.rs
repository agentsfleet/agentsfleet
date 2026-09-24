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

use std::str::FromStr;

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};
use crate::provider::ProviderRegistry;

mod signature;

pub use self::signature::WebhookSignature;

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
/// See [`REASON_SET_EMPTY`].
const REASON_DUPLICATE_MENTION: &str = "a fleet may attach to only one channel";

/// Why a webhook trigger was refused.
const REASON_NO_SOURCE: &str = "a webhook trigger names no source";
/// Why a cron trigger was refused.
const REASON_NO_SCHEDULE: &str = "a cron trigger names no schedule";
/// Why a mention trigger was refused.
const REASON_MENTION_NO_SOURCE: &str = "a mention trigger names no source";
/// See [`REASON_MENTION_NO_SOURCE`].
const REASON_MENTION_NO_CHANNEL: &str = "a mention trigger's `channels` names no channel";
/// Why a channel identifier was refused.
const REASON_NOT_CHANNEL_ID: &str = "a mention trigger's `channels` entry is not a channel identifier: \
     `C` or `G`, then at least eight upper-case letters or digits";

/// The first byte a public channel's identifier carries.
const CHANNEL_PUBLIC: u8 = b'C';
/// The first byte a private channel's identifier carries. A direct message's
/// starts with `D` and is refused: a fleet answers a channel, never one person.
const CHANNEL_PRIVATE: u8 = b'G';
/// The fewest characters after the leading kind byte.
const CHANNEL_MIN_BODY: usize = 8;

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

/// A chat channel, by the identifier its provider minted.
///
/// The identifier rather than the name, because a channel is renamed and its
/// identifier is not: a subscription that followed the name would move to
/// whichever channel took it next. Built only by [`FromStr`], so a value of
/// this type is one the shape check passed (`M-STRONG-TYPES-GUARD`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelId(Box<str>);

impl ChannelId {
    /// The identifier as its provider spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ChannelId {
    type Err = Error;

    /// `C` or `G`, then at least [`CHANNEL_MIN_BODY`] upper-case ASCII letters
    /// or digits, and nothing else.
    ///
    /// # Errors
    /// [`Error::InvalidTriggerSet`] for any other shape, a direct message's
    /// `D…` identifier included.
    fn from_str(candidate: &str) -> Result<Self> {
        let well_formed = match candidate.as_bytes().split_first() {
            Some((&(CHANNEL_PUBLIC | CHANNEL_PRIVATE), body)) => {
                body.len() >= CHANNEL_MIN_BODY
                    && body
                        .iter()
                        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            }
            _ => false,
        };
        if well_formed {
            Ok(Self(candidate.into()))
        } else {
            Err(ErrorKind::InvalidTriggerSet {
                reason: REASON_NOT_CHANNEL_ID,
            }
            .into())
        }
    }
}

/// A fleet woken when someone mentions the bot in one channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// Which chat provider.
    pub source: Box<str>,
    /// The one channel it answers in.
    pub channel: ChannelId,
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
    /// A mention in one chat channel.
    Mention(Mention),
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
                let source = source.filter(|value| !value.is_empty()).ok_or(Error::from(
                    ErrorKind::InvalidTriggerSet {
                        reason: REASON_NO_SOURCE,
                    },
                ))?;

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
                    .ok_or(Error::from(ErrorKind::InvalidTriggerSet {
                        reason: REASON_NO_SCHEDULE,
                    }))?
                    .into(),
                timezone: timezone
                    .unwrap_or_else(|| DEFAULT_CRON_TIMEZONE.to_owned())
                    .into(),
                message: message
                    .unwrap_or_else(|| DEFAULT_CRON_MESSAGE.to_owned())
                    .into(),
            })),
            raw::Trigger::Api => Ok(Self::Api),
            raw::Trigger::Mention { source, channels } => {
                let refuse = |reason| Error::from(ErrorKind::InvalidTriggerSet { reason });
                let source = source
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| refuse(REASON_MENTION_NO_SOURCE))?;
                // The schema allowed exactly one entry, so the first is the
                // only one; absent is the case left to refuse here.
                let channel = channels
                    .and_then(|named| named.into_iter().next())
                    .ok_or_else(|| refuse(REASON_MENTION_NO_CHANNEL))?
                    .parse()?;
                Ok(Self::Mention(Mention {
                    source: source.into(),
                    channel,
                }))
            }
        }
    }

    /// The source this trigger answers to, for the uniqueness check.
    fn source(&self) -> Option<&str> {
        match self {
            Self::Webhook(hook) => Some(&hook.source),
            Self::Cron(_) | Self::Api | Self::Mention(_) => None,
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
    let refuse = |reason| Error::from(ErrorKind::InvalidTriggerSet { reason });

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
    let refuse = |reason| Error::from(ErrorKind::InvalidTriggerSet { reason });

    triggers
        .iter()
        .enumerate()
        .try_fold((), |(), (index, trigger)| {
            let clashes = triggers.iter().skip(index + 1).any(|later| {
                match (trigger, later) {
                    (Trigger::Cron(_), Trigger::Cron(_))
                    | (Trigger::Api, Trigger::Api)
                    | (Trigger::Mention(_), Trigger::Mention(_)) => true,
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
                Trigger::Mention(_) => REASON_DUPLICATE_MENTION,
            }))
        })
}

#[cfg(test)]
#[path = "trigger/tests.rs"]
mod tests;
