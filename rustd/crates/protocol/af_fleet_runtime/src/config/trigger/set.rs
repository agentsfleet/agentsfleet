//! The trigger set as a whole: its uniqueness, and which route can
//! deliver to each webhook in it.

use super::Trigger;
use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

/// Why a trigger set was refused.
const REASON_DUPLICATE_SOURCE: &str = "two webhook triggers share one source";
/// See [`REASON_DUPLICATE_SOURCE`].
const REASON_DUPLICATE_CRON: &str = "a fleet may hold only one cron trigger";
/// See [`REASON_DUPLICATE_SOURCE`].
const REASON_DUPLICATE_MENTION: &str = "a fleet may attach to only one channel";
/// Builds the whole trigger set and proves it is coherent.
///
/// An empty set is a fleet woken only by the messages sent to it.
///
/// # Errors
/// [`Error::InvalidTriggerSet`] for a uniqueness failure, or whichever rule
/// an individual trigger broke. Uniqueness is what bounds the count: one cron,
/// one channel, and one webhook per verified source.
pub(crate) fn parse_set(authored: Vec<raw::Trigger>) -> Result<Box<[Trigger]>> {
    let triggers = authored
        .into_iter()
        .map(Trigger::parse)
        .collect::<Result<Box<[Trigger]>>>()?;

    prove_unique(&triggers)?;
    Ok(triggers)
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
