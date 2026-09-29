//! Who lost a subscription, and when the hub stops waiting to find out.
//!
//! redis-rs 1.7.0 forwards a node's `Disconnection` push WITHOUT the node's
//! address (`cluster_async/mod.rs:209-223`, the push task strips it before
//! handing the push on), so the hub cannot ask which channels died. Nor can it
//! lean on the driver's own replay: the driver repairs the node's socket, but
//! its subscription tracker folds every sharded channel into ONE
//! `SSUBSCRIBE a b …` (`subscription_tracker.rs:105-127`), which the node
//! repair routes by the first channel's slot and drops when that slot is not
//! the repaired node's (`mod.rs:1173-1186`) — and which Dragonfly refuses as
//! `CROSSSLOT` when it does arrive. A hub holding channels on two primaries is
//! never replayed.
//!
//! So the hub re-subscribes every channel it holds itself, one command per
//! channel on the connection it already has, and each confirmation is a gap.
//! A disconnect is EXPLAINED by the first of those confirmations.
//!
//! ```text
//!   Disconnection ──► re-subscribe every channel, wait up to NODE_REPAIR_WINDOW
//!                        ├─ a confirmation arrives ───────────► node repaired,
//!                        │                                      nothing redialled
//!                        ├─ a second Disconnection arrives ───► redial whole
//!                        └─ the window closes ────────────────► redial whole
//! ```
//!
//! Over-reporting is the safe direction: every channel is gapped, not only the
//! lost node's, and a viewer that backfills needlessly loses nothing.
//! Under-reporting is the one outcome this module exists to prevent.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use tokio::time::Instant;

/// How long a lost node socket may go unexplained before the hub redials.
///
/// The driver's repair starts at once and backs off from 50 ms to one second
/// between dials (`reconnect_loop`), so a socket the server merely closed is
/// back in milliseconds and a restarted process in a few seconds; the hub's
/// re-subscribes are confirmed as soon as it is. Five seconds covers the
/// restart; past it the whole-connection redial this hub always had is the
/// fallback, and it too ends in a gap on every channel.
pub(super) const NODE_REPAIR_WINDOW: Duration = Duration::from_secs(5);

/// How long one `hub_channel_gap` warning gathers channels before it is
/// written. A node repair replays its channels one confirmation at a time, and
/// a warning per channel would bury the one fact an operator needs: how many.
pub(super) const GAP_LOG_BURST: Duration = Duration::from_secs(1);

/// Why a channel's subscription was re-issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Cause {
    /// A node's socket was lost and the hub re-subscribed it in place.
    NodeRepaired,
    /// Its slot moved and the hub re-subscribed it on the new owner.
    SlotMoved,
    /// The hub redialled its whole connection.
    Reconnected,
    /// Re-issued for no reason the hub can name — the driver's own replay
    /// after a slot refresh, when it happens to route, for one.
    Resubscribed,
}

impl Cause {
    /// The `cause` field of `hub_channel_gap`.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::NodeRepaired => "node_repaired",
            Self::SlotMoved => "slot_moved",
            Self::Reconnected => "reconnected",
            Self::Resubscribed => "resubscribed",
        }
    }
}

/// Why the whole connection has to be redialled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Loss {
    /// A node's socket died and no confirmation explained it in the window.
    Unexplained,
    /// A second node's socket died while the first was still unexplained.
    Simultaneous,
    /// The driver stopped delivering pushes at all.
    Closed,
    /// A subscribe or unsubscribe the control task issued failed.
    CommandFailed,
}

impl Loss {
    /// The `cause` field of `hub_connection_dropped`.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Unexplained => "unexplained",
            Self::Simultaneous => "simultaneous",
            Self::Closed => "closed",
            Self::CommandFailed => "command_failed",
        }
    }
}

/// What the dispatch task knows about the losses it has seen.
#[derive(Debug, Default)]
pub(super) struct Attribution {
    /// When an unexplained node loss stops being waited on.
    repair: Option<Instant>,
    /// Channels the hub re-subscribed because a node's socket was lost.
    repairing: HashSet<String>,
    /// Channels the hub re-subscribed because their slot moved.
    moved: HashSet<String>,
    /// Channels a redial re-subscribed, confirmed as this connection's first
    /// act.
    redialled: HashSet<String>,
}

impl Attribution {
    /// A fresh connection's view: `redialled` is what it re-subscribes because
    /// the last connection was lost, empty for the first.
    pub(super) fn new(redialled: Vec<String>) -> Self {
        Self {
            redialled: redialled.into_iter().collect(),
            ..Self::default()
        }
    }

    /// A node's socket died while the hub held `live`, which it is about to
    /// re-subscribe. `Some` means stop and redial now instead.
    pub(super) fn disconnected(&mut self, now: Instant, live: &[String]) -> Option<Loss> {
        if self.repair.is_some() {
            return Some(Loss::Simultaneous);
        }
        self.repair = Some(now + NODE_REPAIR_WINDOW);
        self.repairing.extend(live.iter().cloned());
        None
    }

    /// The hub is about to re-subscribe `channel` because its slot moved.
    pub(super) fn moved(&mut self, channel: String) {
        self.moved.insert(channel);
    }

    /// `channel` was confirmed again. Names the cause, and a confirmation
    /// while a node loss is pending is what explains it.
    pub(super) fn replayed(&mut self, channel: &str) -> Cause {
        if self.moved.remove(channel) {
            return Cause::SlotMoved;
        }
        if self.redialled.remove(channel) {
            return Cause::Reconnected;
        }
        let repaired = self.repairing.remove(channel);
        if self.repair.take().is_some() || repaired {
            return Cause::NodeRepaired;
        }
        Cause::Resubscribed
    }

    /// When the pending loss stops being waited on, if one is pending.
    pub(super) const fn deadline(&self) -> Option<Instant> {
        self.repair
    }

    /// `Some` when the pending loss's window has closed unexplained.
    pub(super) fn expired(&mut self, now: Instant) -> Option<Loss> {
        let deadline = self.repair?;
        (now >= deadline).then(|| {
            self.repair = None;
            Loss::Unexplained
        })
    }
}

/// The channels gapped since the current warning opened, by cause.
#[derive(Debug, Default)]
pub(super) struct Burst {
    opened: Option<Instant>,
    tally: BTreeMap<Cause, usize>,
}

impl Burst {
    /// Counts one gapped channel.
    pub(super) fn record(&mut self, cause: Cause, now: Instant) {
        self.opened.get_or_insert(now);
        *self.tally.entry(cause).or_insert(0) += 1;
    }

    /// When the open warning is due, if one is open.
    pub(super) fn due(&self) -> Option<Instant> {
        self.opened.map(|opened| opened + GAP_LOG_BURST)
    }

    /// Closes the open warning, returning what it gathered.
    pub(super) fn take(&mut self) -> BTreeMap<Cause, usize> {
        self.opened = None;
        std::mem::take(&mut self.tally)
    }

    /// Writes and closes the open warning: one line per cause, counts only —
    /// never a channel name, which is a fleet id.
    pub(super) fn flush(&mut self) {
        for (cause, channels) in self.take() {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let error_code = afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str();
            let cause = cause.as_str();
            tracing::warn!(channels, cause, error_code, event = "hub_channel_gap");
        }
    }
}

#[cfg(test)]
mod tests;
