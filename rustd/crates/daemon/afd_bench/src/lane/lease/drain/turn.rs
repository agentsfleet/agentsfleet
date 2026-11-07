//! One simulated turn: what a lease sends between its grant and its report.
//!
//! A drained "chat" that leased and reported at once left out most of a real
//! one: a runner renews its lease every [`RENEWAL_TICK`] and its activity pump
//! posts the live tail every [`ACTIVITY_FLUSH`] while the model streams
//! (`M241_001` §4). The plan counts how many of each one turn of a given
//! length sends, and whether the runner also holds the lease for that long.

use core::time::Duration;

use af_core::timing::RENEWAL_TICK;

/// How often the runner's activity pump flushes a batch of frames: the
/// supervisor's `FLUSH_EVERY`, restated because a bench must not depend on
/// the runner crate it stands in for.
pub(super) const ACTIVITY_FLUSH: Duration = Duration::from_millis(250);

/// What one lease sends before its report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::lane::lease) struct Turn {
    /// Renewals sent through the production renew path.
    pub(super) renewals: u64,
    /// Activity posts sent through the production activity path.
    pub(super) posts: u64,
    /// How long the runner holds the lease before reporting, when it does.
    pub(super) hold: Option<Duration>,
}

impl Turn {
    /// The turn's renewals and posts, sent back to back with no wait, so a
    /// drain window still holds while the statement count covers a whole chat.
    #[must_use]
    pub(in crate::lane::lease) fn back_to_back(turn: Duration) -> Self {
        Self {
            renewals: whole(turn, RENEWAL_TICK),
            posts: whole(turn, ACTIVITY_FLUSH),
            hold: None,
        }
    }

    /// The same sends, and the lease held for the whole turn before the
    /// report, so a fleet is busy while new messages arrive (§5).
    #[must_use]
    pub(in crate::lane::lease) fn held(turn: Duration) -> Self {
        Self {
            hold: Some(turn),
            ..Self::back_to_back(turn)
        }
    }
}

/// Whole intervals of `every` inside `turn`; a partial interval sends nothing.
fn whole(turn: Duration, every: Duration) -> u64 {
    let every = every.as_millis();
    if every == 0 {
        return 0;
    }
    u64::try_from(turn.as_millis() / every).unwrap_or(u64::MAX)
}
