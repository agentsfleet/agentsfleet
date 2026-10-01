//! One workspace's wall: the fleets it carries, and the tick that keeps the
//! set — and the caller's right to it — honest.
//!
//! # Two things go stale while a tab is open, not one
//!
//! A fleet installed after the connection opened has to appear on the wall, and
//! an operator removed from the workspace has to stop seeing it. The first is
//! the reason the daemon this ports has a refresh tick at all; the second is
//! the reason the tick re-authorizes rather than only re-enumerating. They run
//! on ONE beat so a new fleet and a revoked member surface together.
//!
//! # A datastore blip must not close live streams
//!
//! A tick that cannot reach Postgres keeps serving the set it already has and
//! asks again on the next beat. Ending the stream would turn a two-second
//! outage into every dashboard in the fleet reconnecting at once — which is the
//! load the outage was already about.
//!
//! # A lagging viewer re-reads the counters at most once per beat
//!
//! A `catching_up` frame means the counters a tile shows may be stale, so the
//! wall re-announces the set with fresh figures. A viewer falling behind
//! falls behind repeatedly, and one read per lag frame put a slow tab's
//! backlog straight onto Postgres. [`Recount`] spaces those reads a beat
//! apart: the `catching_up` frame itself is still forwarded at once, and only
//! the figures wait.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use afd_auth::principal::Principal;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_sse::{FanIn, Frame, KIND_CATCHING_UP};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use tokio::time::Instant;

use crate::services::{Services, WorkspaceFleets as _, WorkspaceOwnership as _};

/// How often the fleet set and the caller's membership are re-read.
///
/// The cadence `FleetSetCache` runs on, and the same value the store's own
/// cache ages entries at — so a tick either finds a fresh enumeration or is the
/// one viewer whose miss runs the statement for every other viewer.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);

/// The log event a `hello` emits when it goes out without its figures.
const EVENT_HELLO_COUNTERS_UNREAD: &str = "workspace_stream_hello_counters_unread";

/// What one refresh tick concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tick {
    /// The attached set already matches, and the caller still belongs.
    Steady,
    /// Channels were attached, detached, or both.
    Changed,
    /// The caller may no longer read this workspace. The stream sends
    /// `access_revoked` and closes.
    Revoked,
}

/// Everything one workspace stream carries between frames.
struct Wall<D> {
    services: Arc<D>,
    workspace: Uuid7,
    /// Held so the tick can re-ask THIS caller's membership, which is a
    /// question no other viewer's answer can stand in for.
    principal: Principal,
    fan_in: FanIn,
    next_refresh: Instant,
    /// Whether the opening `hello` has been sent.
    announced: bool,
    /// Whether `access_revoked` has been sent, after which nothing else is.
    closed: bool,
    /// When a lag may next re-read the counters.
    recount: Recount,
}

/// Paces the counter reads a lag asks for: at most one per
/// [`REFRESH_INTERVAL`].
///
/// Only a lag's reads are paced. The opening `hello` and a changed set's do
/// not arm the pace, because frames lost after them moved the very counters
/// they announced: the first gap on a fresh connection re-announces at once.
/// Every `hello` still pays off a read a lag was owed.
#[derive(Debug, Clone, Copy)]
struct Recount {
    /// The earliest moment the counters may be read again.
    after: Instant,
    /// Whether a lag asked for a read the pace has not allowed yet.
    owed: bool,
}

impl Recount {
    /// A pace with nothing read yet.
    const fn new(now: Instant) -> Self {
        Self {
            after: now,
            owed: false,
        }
    }

    /// A lag's read is being taken now: the next waits a whole interval.
    fn paced(&mut self, now: Instant) {
        self.after = now + REFRESH_INTERVAL;
        self.owed = false;
    }

    /// A `hello` read the counters, which settles anything a lag was owed.
    const fn paid(&mut self) {
        self.owed = false;
    }

    /// A lag arrived. `true` means read the counters now, and the pace is
    /// armed; otherwise the read is owed at [`Recount::deadline`].
    fn lagged(&mut self, now: Instant) -> bool {
        if now >= self.after {
            self.paced(now);
            return true;
        }
        self.owed = true;
        false
    }

    /// When an owed read falls due, if one is owed.
    fn deadline(&self) -> Option<Instant> {
        self.owed.then_some(self.after)
    }

    /// Whether an owed read is due at `now`.
    fn due(&self, now: Instant) -> bool {
        self.owed && now >= self.after
    }
}

/// Every frame one workspace stream sends, starting with its `hello`.
pub(super) fn frames<D: Services>(
    services: Arc<D>,
    workspace: Uuid7,
    principal: Principal,
    opening: &BTreeSet<String>,
) -> BoxStream<'static, Frame> {
    let mut fan_in = services.live().fan_in();
    fan_in.sync_to(opening);
    let now = Instant::now();
    let wall = Wall {
        services,
        workspace,
        principal,
        fan_in,
        next_refresh: now + REFRESH_INTERVAL,
        announced: false,
        closed: false,
        recount: Recount::new(now),
    };
    stream::unfold(wall, step).boxed()
}

/// The next frame, and the wall that produced it.
async fn step<D: Services>(mut wall: Wall<D>) -> Option<(Frame, Wall<D>)> {
    if wall.closed {
        return None;
    }
    // The set is announced before any activity, so a client knows which tiles
    // to open before the first frame arrives for one of them.
    if !wall.announced {
        wall.announced = true;
        return Some(announce(wall).await);
    }
    loop {
        if Instant::now() >= wall.next_refresh {
            match refresh(&mut wall).await {
                Tick::Revoked => {
                    wall.closed = true;
                    let refused = error_code::AUTH_FORBIDDEN.as_str();
                    return Some((Frame::access_revoked(refused), wall));
                }
                Tick::Changed => return Some(announce(wall).await),
                Tick::Steady => {}
            }
        }
        let now = Instant::now();
        if wall.recount.due(now) {
            wall.recount.paced(now);
            return Some(announce(wall).await);
        }
        // Wake for whichever comes first. Sleeping the whole beat would be
        // fine, but waking on the frame is what keeps latency at the
        // publisher's rather than at the tick's.
        let deadline = wall
            .recount
            .deadline()
            .map_or(wall.next_refresh, |owed| owed.min(wall.next_refresh));
        let arrived = tokio::select! {
            frame = wall.fan_in.next_frame() => Some(frame),
            () = tokio::time::sleep_until(deadline) => None,
        };
        if let Some(frame) = arrived {
            // A gap the server could not carry is exactly the frames that
            // moved the counters, so the set is re-announced with where every
            // fleet stands now — the backfill recovers the rows, the fresh
            // `hello` recovers the figures — at the pace `Recount` allows.
            if frame.kind == KIND_CATCHING_UP && wall.recount.lagged(Instant::now()) {
                wall.announced = false;
            }
            return Some((frame, wall));
        }
    }
}

/// The `hello` for the set the wall carries now, which reads the counters.
async fn announce<D: Services>(mut wall: Wall<D>) -> (Frame, Wall<D>) {
    wall.recount.paid();
    let carried = wall.fan_in.fleets();
    let frame = hello(wall.services.as_ref(), &wall.workspace, carried).await;
    (frame, wall)
}

/// The `hello` for the set the wall carries now, with where each fleet stands.
///
/// The counters are read fresh for every `hello` — on connect and on a change
/// to the set, never on a steady tick — and best-effort: a read that does not
/// answer sends the set without its figures, and a client leaves what it has
/// standing. Zeros would say the fleet has done nothing, which is the one
/// thing a failed read does not know.
///
/// Takes the wall's parts rather than the wall: the fan-in is not `Sync`, and
/// a borrow of the whole held across this read would make the stream's future
/// unsendable.
async fn hello<D: Services>(services: &D, workspace: &Uuid7, carried: Vec<String>) -> Frame {
    let counters = match services.fleets().counters(workspace, &carried).await {
        Ok(counters) => counters,
        Err(error) => {
            let workspace_id = workspace.as_str();
            let error_code = error.code().as_str();
            let reason = error.to_string();
            tracing::warn!(
                workspace_id,
                error_code,
                reason,
                event = EVENT_HELLO_COUNTERS_UNREAD
            );
            BTreeMap::new()
        }
    };
    Frame::hello(&carried, &counters)
}

/// Re-authorize the caller, then align the attached set with the workspace's.
async fn refresh<D: Services>(wall: &mut Wall<D>) -> Tick {
    wall.next_refresh = Instant::now() + REFRESH_INTERVAL;

    match wall
        .services
        .workspaces()
        .authorize(&wall.principal, &wall.workspace)
        .await
    {
        Ok(Some(_access)) => {}
        Ok(None) => {
            // Detach before returning, so no frame already queued on an
            // attached channel can still reach a caller who lost the right
            // to it.
            wall.fan_in.sync_to(&BTreeSet::new());
            let workspace_id = wall.workspace.as_str();
            tracing::debug!(workspace_id, event = "workspace_stream_revoked");
            return Tick::Revoked;
        }
        Err(_deferred) => return Tick::Steady,
    }

    match wall.services.fleets().live_set(&wall.workspace).await {
        Ok(fleets) => {
            if wall.fan_in.sync_to(&fleets).is_change() {
                Tick::Changed
            } else {
                Tick::Steady
            }
        }
        Err(_deferred) => Tick::Steady,
    }
}

#[cfg(test)]
mod tests;
