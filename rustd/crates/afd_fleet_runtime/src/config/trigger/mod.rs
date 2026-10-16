//! What may wake a fleet.
//!
//! # Why the set is validated, not just each entry
//!
//! Two webhook triggers on one source, or two cron entries, are each
//! individually well-formed and jointly meaningless: the fleet would be woken
//! twice for one delivery, or run twice on one schedule. The check runs over
//! already-typed values, so the comparison is a `match` the compiler proves
//! total rather than a reach into a variant whose tag was checked one line
//! earlier.

use std::str::FromStr;

use afd_validate::charset;
use afd_webhook::Scheme;
use garde::{Unvalidated, Valid};

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

mod set;
mod signature;

pub(crate) use self::set::parse_set;
pub use self::signature::WebhookSignature;

/// Where a cron trigger's schedule is read, when it names no zone.
const DEFAULT_CRON_TIMEZONE: &str = "UTC";
/// What a scheduled run is told it is for, when it says nothing.
const DEFAULT_CRON_MESSAGE: &str = "Scheduled Fleet run";

/// The `type` a webhook trigger is written with.
pub const KIND_WEBHOOK: &str = "webhook";
/// The `type` a cron trigger is written with.
pub const KIND_CRON: &str = "cron";
/// The `type` a mention trigger is written with.
pub const KIND_MENTION: &str = "mention";
/// Every `type` the parser accepts, in the order `raw::Trigger` declares them.
///
/// That is also the order serde names them in when it refuses any other.
/// Serde's rename is a literal it cannot take from a constant, so
/// `every_trigger_kind_is_one_the_parser_accepts` pins the two together.
pub const TRIGGER_KINDS: [&str; 3] = [KIND_WEBHOOK, KIND_CRON, KIND_MENTION];

/// The webhook source GitHub's own route serves, `/v1/webhooks/{fleet_id}/github`.
pub const WEBHOOK_SOURCE_GITHUB: &str = Scheme::BodyHex.source();
/// The webhook source Linear's signature scheme is named by.
pub const WEBHOOK_SOURCE_LINEAR: &str = Scheme::BodyHexBare.source();
/// The webhook source Slack's signature scheme is named by.
pub const WEBHOOK_SOURCE_SLACK: &str = Scheme::SlackV0.source();
/// Every webhook source whose signature scheme this daemon ships.
///
/// In `Scheme::ALL`'s order, and sized by it, so a new scheme does not compile
/// until it is named here; `verified_sources_are_the_schemes` pins the names.
pub const VERIFIED_WEBHOOK_SOURCES: [&str; Scheme::ALL.len()] = [
    WEBHOOK_SOURCE_GITHUB,
    WEBHOOK_SOURCE_LINEAR,
    WEBHOOK_SOURCE_SLACK,
];
/// The source no webhook trigger may take: its URL segment is the
/// approval-decision route's.
pub const WEBHOOK_SOURCE_APPROVAL: &str = "approval";

/// Most triggers one fleet may declare.
pub(crate) const MAX_TRIGGERS: usize = 8;

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
/// The fewest characters a channel identifier carries: its kind byte and body.
pub(crate) const CHANNEL_MIN_LEN: usize = 1 + CHANNEL_MIN_BODY;

/// A channel identifier as authored, bounded before its kind byte is read.
#[derive(Debug, garde::Validate)]
struct Candidate<'a> {
    #[garde(length(bytes, min = CHANNEL_MIN_LEN), custom(charset(is_channel_char)))]
    id: &'a str,
}

/// Whether `character` may appear in a channel identifier: upper-case ASCII
/// letters and digits, the kind byte included.
const fn is_channel_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit()
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
    /// The Svix secret, for a source verified on the Svix route.
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
        Unvalidated::new(Candidate { id: candidate })
            .validate()
            .ok()
            .and_then(|proved| Self::read(&proved))
            .ok_or_else(|| {
                ErrorKind::InvalidTriggerSet {
                    reason: REASON_NOT_CHANNEL_ID,
                }
                .into()
            })
    }
}

impl ChannelId {
    /// The identifier, when a proved candidate opens with a channel's kind
    /// byte; a direct message's `D` does not.
    fn read(candidate: &Valid<Candidate<'_>>) -> Option<Self> {
        matches!(
            candidate.id.as_bytes().first(),
            Some(&(CHANNEL_PUBLIC | CHANNEL_PRIVATE))
        )
        .then(|| Self(candidate.id.into()))
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

/// What may wake a fleet besides a message sent to it.
///
/// A steer message reaches every fleet whatever it declares here, so a fleet
/// that is only ever talked to declares no trigger at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// A signed delivery from an external provider.
    Webhook(Webhook),
    /// A schedule.
    Cron(Cron),
    /// A mention in one chat channel.
    Mention(Mention),
}

impl Trigger {
    /// Builds one trigger.
    ///
    /// A webhook trigger must be one some route can verify: its source names a
    /// signature scheme this daemon ships, or it declares a Svix secret. A
    /// trigger neither route could check would be handed a URL every delivery
    /// to fails. Mention triggers share the `source` key and not this rule.
    ///
    /// # Errors
    /// Whichever rule the trigger broke.
    fn parse(authored: raw::Trigger) -> Result<Self> {
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

                let signature = signature
                    .map(|block| WebhookSignature::resolve(block, &source))
                    .transpose()?;
                if Scheme::for_source(&source).is_none() && signature.is_none() {
                    return Err(ErrorKind::UnverifiableWebhookSource {
                        provider: source.into(),
                    }
                    .into());
                }

                Ok(Self::Webhook(Webhook {
                    // Both lists were bounded by the schema, so what is left
                    // is ownership.
                    events: events.map(own),
                    repositories: repositories.map(own),
                    signature,
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
}

/// Takes ownership of an already-bounded list.
fn own(items: Vec<String>) -> Box<[Box<str>]> {
    items.into_iter().map(Into::into).collect()
}

#[cfg(test)]
mod tests;
