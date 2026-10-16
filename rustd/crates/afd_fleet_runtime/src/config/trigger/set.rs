//! The trigger set as a whole: its size, its uniqueness, and which route can
//! deliver to each webhook in it.

use afd_webhook::Scheme;
use garde::{Unvalidated, Valid};

use super::{MAX_TRIGGERS, Trigger};
use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

/// Why a trigger set was refused.
pub(super) const REASON_SET_TOO_LARGE: &str = "it holds more triggers than the limit";
/// See [`REASON_SET_TOO_LARGE`].
const REASON_DUPLICATE_SOURCE: &str = "two webhook triggers share one source";
/// See [`REASON_SET_TOO_LARGE`].
const REASON_DUPLICATE_CRON: &str = "a fleet may hold only one cron trigger";
/// See [`REASON_SET_TOO_LARGE`].
const REASON_DUPLICATE_MENTION: &str = "a fleet may attach to only one channel";
/// See [`REASON_SET_TOO_LARGE`].
pub(super) const REASON_SVIX_NOT_FIRST: &str = "a Svix-signed webhook source must be the fleet's first \
     webhook trigger: the Svix route delivers only to the first";

/// The authored trigger set, bounded in count before any entry is read.
#[derive(Debug, garde::Validate)]
struct Set {
    #[garde(length(max = MAX_TRIGGERS))]
    authored: Vec<raw::Trigger>,
}

/// Builds the whole trigger set and proves it is coherent.
///
/// An empty set is a fleet woken only by the messages sent to it.
///
/// # Errors
/// [`Error::InvalidTriggerSet`] for a count or uniqueness failure, or
/// whichever rule an individual trigger broke.
pub(crate) fn parse_set(authored: Vec<raw::Trigger>) -> Result<Box<[Trigger]>> {
    let refuse = |reason| Error::from(ErrorKind::InvalidTriggerSet { reason });

    let set = Unvalidated::new(Set { authored })
        .validate()
        .map_err(|_too_many| refuse(REASON_SET_TOO_LARGE))?;
    read_set(set)
}

/// Reads each entry of a set whose count is proved, then proves the whole is
/// coherent.
fn read_set(set: Valid<Set>) -> Result<Box<[Trigger]>> {
    let triggers = set
        .into_inner()
        .authored
        .into_iter()
        .map(Trigger::parse)
        .collect::<Result<Box<[Trigger]>>>()?;

    prove_unique(&triggers)?;
    prove_svix_first(&triggers).map(|()| triggers)
}

/// Proves every Svix-signed webhook is one the Svix route can reach.
///
/// That route's URL carries only the fleet id, so the ingress resolves the
/// fleet's first webhook trigger (`afd_ingress` `webhook_trigger` with no
/// source). A Svix-signed source anywhere else would be advertised an address
/// whose deliveries bind to another trigger and are refused, so it is refused
/// here instead. A source with a scheme of its own has its own route.
fn prove_svix_first(triggers: &[Trigger]) -> Result<()> {
    let misplaced = triggers
        .iter()
        .filter_map(|trigger| match trigger {
            Trigger::Webhook(hook) => Some(hook),
            Trigger::Cron(_) | Trigger::Mention(_) => None,
        })
        .skip(1)
        .any(|hook| Scheme::for_source(&hook.source).is_none());

    if misplaced {
        return Err(ErrorKind::InvalidTriggerSet {
            reason: REASON_SVIX_NOT_FIRST,
        }
        .into());
    }
    Ok(())
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
                    | (Trigger::Mention(_), Trigger::Mention(_)) => true,
                    // Two webhooks clash only on one source. Different sources
                    // are the whole point of declaring more than one.
                    (Trigger::Webhook(first), Trigger::Webhook(second)) => {
                        first.source == second.source
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
                Trigger::Mention(_) => REASON_DUPLICATE_MENTION,
            }))
        })
}
