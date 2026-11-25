//! The trigger set as a whole: its uniqueness, and which route can
//! deliver to each webhook in it.

use garde::Validate as _;

use super::Trigger;
use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

/// Why a trigger set was refused.
const REASON_DUPLICATE_SOURCE: &str = "two webhook triggers share one source";
/// See [`REASON_DUPLICATE_SOURCE`].
const REASON_DUPLICATE_CRON: &str = "a fleet may hold only one cron trigger";
/// See [`REASON_DUPLICATE_SOURCE`].
const REASON_DUPLICATE_MENTION: &str = "a fleet may attach to only one channel";
/// Reads a trigger list — a JSON array of triggers, each `{"type": …}` with
/// its kind's keys — and proves the set is coherent.
///
/// An empty list is a fleet woken only by the messages sent to it.
///
/// # Errors
/// A shape failure or an unknown key, a bound a trigger broke, whichever rule
/// one trigger broke, or `InvalidTriggerSet` for a uniqueness failure.
/// Uniqueness is what bounds the count: one cron, one channel, and one webhook
/// per source.
pub fn parse_triggers(document: &str) -> Result<Box<[Trigger]>> {
    let authored: Vec<raw::Trigger> = raw::strict(document)?;
    authored.validate()?;
    parse_set(authored)
}

/// Builds every trigger of an already-bounded list and proves the set.
pub(super) fn parse_set(authored: Vec<raw::Trigger>) -> Result<Box<[Trigger]>> {
    let triggers = authored
        .into_iter()
        .map(Trigger::parse)
        .collect::<Result<Box<[Trigger]>>>()?;

    prove_unique(&triggers)?;
    Ok(triggers)
}

/// Proves no two triggers would fire for the same thing: one cron, one
/// channel, and one webhook per source.
///
/// Public so a list read from stored rows plus one being added is judged by
/// the same rule as a list read whole.
///
/// # Errors
/// `InvalidTriggerSet` naming the clash.
pub fn prove_unique(triggers: &[Trigger]) -> Result<()> {
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
