//! Re-subscribing after a node's socket was lost, each channel only once its
//! owning primary is the node the driver would send it to.
//!
//! # Why wait for the owner
//!
//! The driver routes a master command to its slot's primary only while that
//! primary's socket is connected. While it is being repaired, the command goes
//! to a RANDOM connected node (redis-rs 1.7.0 `cluster_async/mod.rs:944-969`)
//! — possibly that primary's own replica, and Dragonfly lets a replica accept
//! `SSUBSCRIBE` for its primary's slot, confirm it, and never deliver the
//! primary's `SPUBLISH` to it. A re-subscribe sent the instant the loss is
//! reported is sent exactly then: a gap followed by silence, the one outcome
//! the hub exists to prevent.
//!
//! # Why ask `CLUSTER MYID`, and not route by address
//!
//! A by-address route to a node the driver holds no socket to fails as
//! `ClusterConnectionNotFound`, which the driver answers by reconnecting from
//! its seed nodes — REPLACING its whole connection map (`mod.rs:1464-1477`),
//! every healthy node's subscriptions with it. So the hub never names an
//! address. It sends `CLUSTER MYID` along the very route a subscribe to that
//! slot would take: while the owner is disconnected the random fallback
//! answers with another node's id, and once the answer is the owner's id
//! (from `CLUSTER SLOTS`) the driver holds the owner's socket and the
//! subscribes follow on the same route. An owner lost again in between is a
//! second `Disconnection`, which the dispatch task turns into a redial.
//!
//! # Why every owner, and why the table is read again
//!
//! The push does not say which node was lost, so the repair waits for EVERY
//! range's owner, whether or not the hub holds a channel there. The control
//! task serves nothing else while it waits, so a reader's `Subscribe` queued
//! meanwhile is sent only once the driver routes it to its owner. One sent in
//! the moment before the repair began — after the loss, before the dispatch
//! task's `Repair` reached the control task — went wherever the driver's
//! fallback sent it, and it is not in the list the repair was handed. So once
//! the owners answer, the channel table is read again and every channel that
//! joined it is subscribed too. A channel whose own `Subscribe` is still
//! queued is confirmed twice, a gap its reader did not need: over-reporting,
//! the safe direction the `gap` module chooses.
//!
//! # What the window ending means
//!
//! The owner never answered in time. That covers a primary that is down, and
//! a replica promoted in its place while the driver's slot map still names
//! the old primary. Either way the caller redials, and a fresh connection
//! reads the topology the cluster has now.

use std::collections::HashSet;
use std::time::Duration;

use redis::cluster_async::ClusterConnection;
use redis::cluster_routing::{Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};
use tokio::time::Instant;

use super::channels::HubInner;
use super::gap::{Loss, NODE_REPAIR_WINDOW};
use crate::topology::{self, SlotRange, text};

/// How long to wait before asking again whether an owner is back. The
/// driver's own repair backs off from 50 ms; asking far more often than that
/// only spins.
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// Cluster slots, as the cluster protocol numbers them.
const SLOTS: u16 = 16_384;

/// Waits for every range's owner, then re-subscribes `held` and every
/// channel a reader took up since `held` was read.
///
/// # Errors
/// [`Loss::Unexplained`] when an owner is not reachable inside
/// [`NODE_REPAIR_WINDOW`], and [`Loss::CommandFailed`] when the slot map
/// cannot be read, names no owner for a channel, or a subscribe fails — the
/// caller redials either way.
pub(super) async fn resubscribe(
    connection: &mut ClusterConnection,
    inner: &HubInner,
    held: &[String],
) -> Result<(), Loss> {
    let deadline = Instant::now() + NODE_REPAIR_WINDOW;
    let ranges = topology::slot_ranges(connection)
        .await
        .map_err(|_unread| Loss::CommandFailed)?;
    for range in &ranges {
        await_owner(connection, range, deadline).await?;
    }
    let joined = joined(held, inner.live_channels());
    for channel in held.iter().chain(&joined) {
        if !ranges.iter().any(|range| holds(range, slot(channel))) {
            return Err(Loss::CommandFailed);
        }
        connection
            .ssubscribe(channel)
            .await
            .map_err(|_failed| Loss::CommandFailed)?;
    }
    Ok(())
}

/// The channels in `live` that `held` does not name: taken up by a reader
/// after the repair's list was read.
fn joined(held: &[String], live: Vec<String>) -> Vec<String> {
    let held: HashSet<&str> = held.iter().map(String::as_str).collect();
    live.into_iter()
        .filter(|channel| !held.contains(channel.as_str()))
        .collect()
}

/// Waits until a command routed to `range`'s slots is answered by the
/// range's own primary, or `deadline`.
async fn await_owner(
    connection: &mut ClusterConnection,
    range: &SlotRange,
    deadline: Instant,
) -> Result<(), Loss> {
    // A reply naming no id leaves nothing to compare, and the slot's own
    // routing is all there is. A cluster that names its nodes never takes
    // this.
    let Some(owner) = range.id.as_deref() else {
        return Ok(());
    };
    // Along the slot's own route, NEVER by address: a by-address route to a
    // node the driver holds no socket to fails as `ClusterConnectionNotFound`,
    // which the driver answers by reconnecting from its seeds and replacing
    // its whole connection map (redis-rs 1.7.0 `cluster_async/mod.rs:1464-1477`,
    // `errors/redis_error.rs:447`) — every healthy node's subscriptions gone,
    // silently. The module header has the rest.
    let route = RoutingInfo::SingleNode(SingleNodeRoutingInfo::SpecificNode(Route::new(
        range.first,
        SlotAddr::Master,
    )));
    loop {
        let mut cmd = redis::cmd("CLUSTER");
        cmd.arg("MYID");
        let answered = connection.route_command(cmd, route.clone()).await;
        if answered.ok().as_ref().and_then(text).as_deref() == Some(owner) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Loss::Unexplained);
        }
        tokio::time::sleep(RETRY_INTERVAL).await;
    }
}

/// Whether `range` holds `slot`.
const fn holds(range: &SlotRange, slot: u16) -> bool {
    range.first <= slot && slot <= range.last
}

/// The cluster slot `key` hashes to: CRC16 (XMODEM) of the key, or of its
/// hash tag — the text between the first `{` and the next `}` — when that is
/// not empty, modulo the slot count.
fn slot(key: &str) -> u16 {
    let bytes = key.as_bytes();
    let tagged = key.find('{').and_then(|open| {
        let after = bytes.get(open + 1..)?;
        let close = after.iter().position(|byte| *byte == b'}')?;
        after.get(..close).filter(|tag| !tag.is_empty())
    });
    crc16::State::<crc16::XMODEM>::calculate(tagged.unwrap_or(bytes)) % SLOTS
}

#[cfg(test)]
mod tests;
