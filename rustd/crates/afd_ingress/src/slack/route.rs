//! Which one fleet a mention reaches, or which one notice it earns.
//!
//! A pure function of the channel's subscribers and the mention's text, so the
//! routing table in `docs/architecture/scenarios/slack-incident-responder.md`
//! §4 is proven row by row with no datastore. The verdict is a tagged union
//! (RULE TGU): each arm names at most one fleet, so no path through here can
//! admit two events for one mention.

use super::Subscriber;

/// What trails an addressed name and is not part of it: `incident: why?`.
const NAME_TERMINATORS: [char; 2] = [':', ','];

/// Where a mention goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Route<'s, 'm> {
    /// The subscriber the mention named, and the message with that name
    /// removed and the rest kept verbatim.
    Addressed {
        /// The fleet named.
        fleet: &'s Subscriber,
        /// What the person asked it.
        message: &'m str,
    },
    /// The channel's one eligible subscriber, for a mention naming nobody.
    Sole {
        /// The fleet.
        fleet: &'s Subscriber,
        /// The whole message.
        message: &'m str,
    },
    /// Nothing subscribes, so the channel's resident answers.
    Resident {
        /// The whole message.
        message: &'m str,
    },
    /// No model runs; one fixed text answers in the thread.
    Notice(Notice<'s>),
}

/// Why a mention earns a notice rather than a run.
#[derive(Debug, PartialEq, Eq)]
pub enum Notice<'s> {
    /// The first word names two fleets whose names differ only in case.
    Ambiguous {
        /// The fleets it could mean.
        fleets: Vec<&'s Subscriber>,
    },
    /// Several fleets could answer an unaddressed mention.
    Choose {
        /// The fleets a person can address.
        fleets: Vec<&'s Subscriber>,
    },
    /// Only addressed-only or unrunnable fleets are attached, and the mention
    /// named none of them.
    AddressIt {
        /// The attached fleets.
        fleets: Vec<&'s Subscriber>,
    },
    /// The mention named a fleet that cannot run now.
    Paused {
        /// The fleet named.
        fleet: &'s Subscriber,
    },
}

impl Notice<'_> {
    /// The kind's stored spelling, for the operator event and the notice key.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Ambiguous { .. } => "ambiguous",
            Self::Choose { .. } => "choose",
            Self::AddressIt { .. } => "address_it",
            Self::Paused { .. } => "paused",
        }
    }
}

/// Routes one mention whose leading bot mention the caller already removed.
///
/// A first word equal to one subscriber's name, ignoring case and a trailing
/// `:` or `,`, addresses it; equal to two, it is ambiguous. A mention naming
/// no subscriber is unaddressed: nobody attached → the resident; exactly one
/// eligible subscriber (runnable and not addressed-only) → that fleet;
/// several → choose; none → address it.
#[must_use]
pub fn route<'s, 'm>(subscribers: &'s [Subscriber], text: &'m str) -> Route<'s, 'm> {
    let text = text.trim_start();
    let (word, rest) = text
        .split_once(char::is_whitespace)
        .map_or((text, ""), |(word, rest)| (word, rest.trim_start()));
    let name = word.trim_end_matches(NAME_TERMINATORS);

    let named: Vec<&Subscriber> = subscribers
        .iter()
        .filter(|subscriber| !name.is_empty() && subscriber.name.eq_ignore_ascii_case(name))
        .collect();
    match named.as_slice() {
        [fleet] if fleet.runnable => Route::Addressed {
            fleet,
            message: rest,
        },
        [fleet] => Route::Notice(Notice::Paused { fleet }),
        [] => unaddressed(subscribers, text),
        [..] => Route::Notice(Notice::Ambiguous { fleets: named }),
    }
}

/// A mention naming no subscriber.
fn unaddressed<'s, 'm>(subscribers: &'s [Subscriber], text: &'m str) -> Route<'s, 'm> {
    if subscribers.is_empty() {
        return Route::Resident { message: text };
    }
    let eligible: Vec<&Subscriber> = subscribers
        .iter()
        .filter(|subscriber| subscriber.runnable && !subscriber.addressed_only)
        .collect();
    match eligible.as_slice() {
        [fleet] => Route::Sole {
            fleet,
            message: text,
        },
        [] => Route::Notice(Notice::AddressIt {
            fleets: subscribers.iter().collect(),
        }),
        [..] => Route::Notice(Notice::Choose {
            fleets: subscribers
                .iter()
                .filter(|subscriber| subscriber.runnable)
                .collect(),
        }),
    }
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
