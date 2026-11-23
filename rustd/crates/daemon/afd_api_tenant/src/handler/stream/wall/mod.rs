//! One workspace's wall: the fleets it carries, and the notices that keep the
//! set — and the caller's right to it — honest.
//!
//! # Two things go stale while a tab is open, not one
//!
//! A fleet installed after the connection opened has to appear on the wall, and
//! an operator removed from the workspace has to stop seeing it. A fleet create or
//! delete publishes a notice on the workspace after its commit, and a member
//! removal publishes one on the account; either, or a gap that may have hidden
//! one, makes the wall re-authorize and re-enumerate. Nothing re-reads on a timer:
//! an idle wall runs no statement.
//!
//! # A datastore blip must not close live streams
//!
//! A refresh that cannot read the set keeps serving the set it already has, and
//! the next notice reads again. Ending the stream would turn a two-second outage
//! into every dashboard in the fleet reconnecting at once — which is the load the
//! outage was already about. The membership half is the re-check both streams
//! share (`revocable.rs`): an outage there ends the wall without
//! `access_revoked`.
//!
//! # A lagging viewer re-reads the counters at most once per pace
//!
//! A `catching_up` frame means the counters a tile shows may be stale, so the
//! wall re-announces the set with fresh figures. A viewer falling behind
//! falls behind repeatedly, and one read per lag frame put a slow tab's
//! backlog straight onto Postgres. [`Recount`] spaces those reads a pace
//! apart: the `catching_up` frame itself is still forwarded at once, and only
//! the figures wait.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use afd_auth::principal::Principal;
use afd_bus::Subject;
use afd_http::auth::Owned;
use afd_sse::{FanIn, Frame, KIND_CATCHING_UP, Link, Wake};
use futures_util::stream::BoxStream;
use tokio::time::Instant;

use super::revocable::{
    Membership, Recheck as _, STREAM_WALL, Turn, Verdict, Watch, until_revoked,
};
use crate::services::{TenantPlane, WorkspaceFleets as _};

/// The least time between two counter reads a lag asks for. A lag storm on a busy
/// workspace would otherwise read the counters once per lost frame. The number is
/// awaiting the owner's ruling (spec M255_001, Discovery); nothing forces 10 s.
const RECOUNT_PACE: Duration = Duration::from_secs(10);

/// The log event a refresh emits when the workspace's fleet set does not answer.
const EVENT_SET_UNREAD: &str = "workspace_stream_set_unread";

/// The log event a wall emits when the bus shut down under it.
const EVENT_BUS_CLOSED: &str = "workspace_stream_bus_closed";

/// The log event a `hello` emits when it goes out without its figures.
const EVENT_HELLO_COUNTERS_UNREAD: &str = "workspace_stream_hello_counters_unread";

/// What one refresh concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tick {
    /// The attached set already matches, and the caller still belongs.
    Steady,
    /// Channels were attached, detached, or both.
    Changed,
    /// The caller may no longer read this workspace. The stream sends
    /// `access_revoked` and closes.
    Revoked,
    /// The caller's access went unanswered. The stream closes without
    /// `access_revoked`, and the reconnect is authorized at open.
    Unverified,
}

/// Everything one workspace stream carries between frames.
struct Wall<D> {
    /// THIS caller's membership, re-asked on every notice: no other viewer's
    /// answer can stand in for it. Its services and workspace are the ones
    /// the wall reads its set and counters through.
    watch: Watch<Membership<D>>,
    fan_in: FanIn,
    /// The account's members changed: re-ask the caller's membership.
    members: Link,
    /// The workspace gained or lost a fleet: re-read the set.
    fleets: Link,
    /// The reads a wake asked for and the wall has not run yet.
    owed: Owed,
    /// Whether the opening `hello` has been sent.
    announced: bool,
    /// When a lag may next re-read the counters.
    recount: Recount,
}

/// Paces the counter reads a lag asks for: at most one per [`RECOUNT_PACE`].
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
        self.after = now + RECOUNT_PACE;
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

/// The reads a wall owes. Each kind of notice owes only its own read; a gap reaches
/// both links and so owes both.
#[derive(Debug, Clone, Copy, Default)]
struct Owed {
    /// Re-ask the caller's membership.
    access: bool,
    /// Re-read the workspace's fleet set.
    set: bool,
    /// The last set read failed: the next wake of any kind reads it again.
    set_retry: bool,
}

impl Owed {
    /// Both reads, once: the wall authorized and enumerated before its links opened,
    /// so a removal or a fleet published in between is read here.
    const OPENING: Self = Self {
        access: true,
        set: true,
        set_retry: false,
    };

    /// A wake of any kind settles a failed set read.
    const fn woke(&mut self) {
        if self.set_retry {
            self.set_retry = false;
            self.set = true;
        }
    }
}

/// Every frame one workspace stream sends, starting with its `hello`.
pub(super) async fn frames<D: TenantPlane>(
    services: Arc<D>,
    owned: Owned,
    principal: Principal,
    opening: &BTreeSet<String>,
) -> BoxStream<'static, Frame> {
    let Owned { workspace, tenant } = owned;
    let mut fan_in = services.live().fan_in();
    fan_in.sync_to(opening).await;
    let members = services.live().link(Subject::Members(tenant)).await;
    let fleets = services
        .live()
        .link(Subject::Fleets(workspace.clone()))
        .await;
    let now = Instant::now();
    let wall = Wall {
        watch: Watch::new(Membership::new(services, principal, workspace), STREAM_WALL),
        fan_in,
        members,
        fleets,
        owed: Owed::OPENING,
        announced: false,
        recount: Recount::new(now),
    };
    until_revoked(wall, step)
}

/// The next frame, and the wall that produced it.
async fn step<D: TenantPlane>(mut wall: Wall<D>) -> Option<Turn<Wall<D>>> {
    // The set is announced before any activity, so a client knows which tiles
    // to open before the first frame arrives for one of them.
    if !wall.announced {
        wall.announced = true;
        return Some(announce(wall).await);
    }
    loop {
        let now = Instant::now();
        if wall.recount.due(now) {
            wall.recount.paced(now);
            return Some(announce(wall).await);
        }
        match settle(&mut wall).await {
            Tick::Revoked => return Some(Turn::Revoked),
            // An outage is not a revocation: the wall ends with no frame.
            Tick::Unverified => return None,
            Tick::Changed => return Some(announce(wall).await),
            Tick::Steady => {}
        }
        // No timer re-reads Postgres: the wall wakes on a frame, on a notice that the
        // account's members or the workspace's fleets changed, or on a gap or lost
        // subscription that may have hidden one. Only an owed counter read sleeps.
        let woke = tokio::select! {
            frame = wall.fan_in.next_frame() => Woke::Frame(frame),
            wake = wall.members.changed() => Woke::Members(wake),
            wake = wall.fleets.changed() => Woke::Fleets(wake),
            () = afd_sse::sleep_until_some(wall.recount.deadline()) => continue,
        };
        wall.owed.woke();
        match woke {
            Woke::Frame(frame) => {
                // A gap the server could not carry is exactly the frames that
                // moved the counters, so the set is re-announced with where every
                // fleet stands now — the backfill recovers the rows, the fresh
                // `hello` recovers the figures — at the pace `Recount` allows.
                if frame.kind == KIND_CATCHING_UP && wall.recount.lagged(Instant::now()) {
                    wall.announced = false;
                }
                return Some(Turn::Frame(frame, wall));
            }
            // The bus shut down: end, so the client reconnects to a serving replica.
            Woke::Members(Wake::Closed) | Woke::Fleets(Wake::Closed) => {
                tracing::debug!(event = EVENT_BUS_CLOSED);
                return None;
            }
            Woke::Members(Wake::Changed) => wall.owed.access = true,
            Woke::Fleets(Wake::Changed) => wall.owed.set = true,
        }
    }
}

/// What woke the wall.
enum Woke {
    Frame(Frame),
    Members(Wake),
    Fleets(Wake),
}

/// The `hello` for the set the wall carries now, which reads the counters.
async fn announce<D: TenantPlane>(mut wall: Wall<D>) -> Turn<Wall<D>> {
    wall.recount.paid();
    let carried = wall.fan_in.fleets();
    let membership = wall.watch.question();
    let frame = hello(membership.services(), membership.workspace(), carried).await;
    Turn::Frame(frame, wall)
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
async fn hello<D: TenantPlane>(services: &D, workspace: &Uuid7, carried: Vec<String>) -> Frame {
    let counters = match services.fleets().counters(workspace, &carried).await {
        Ok(counters) => counters,
        Err(error) => {
            counters_unread(workspace, &error);
            BTreeMap::new()
        }
    };
    Frame::hello(&carried, &counters)
}

/// A `hello` whose counters did not answer: the set goes out without them.
fn counters_unread(workspace: &Uuid7, error: &afd_fleet_lifecycle::Error) {
    let workspace_id = workspace.as_str();
    let Logged { error_code, reason } = error.logged();
    {
        let event = &EVENT_HELLO_COUNTERS_UNREAD;
        tracing::warn!(workspace_id, error_code, reason, event);
    };
}

/// Runs the reads the wall owes: the caller's membership, then the attached set aligned
/// with the workspace's. A set read that fails keeps the set the wall has, logs, and is
/// read again at the next wake.
async fn settle<D: TenantPlane>(wall: &mut Wall<D>) -> Tick {
    if std::mem::take(&mut wall.owed.access) {
        match wall.watch.verdict().await {
            Verdict::Admitted => {}
            Verdict::Unverified => return Tick::Unverified,
            Verdict::Revoked => {
                // Detach before returning, so no frame already queued on an
                // attached channel can still reach a caller who lost the right
                // to it.
                wall.fan_in.sync_to(&BTreeSet::new()).await;
                return Tick::Revoked;
            }
        }
    }
    if !std::mem::take(&mut wall.owed.set) {
        return Tick::Steady;
    }
    let membership = wall.watch.question();
    let workspace = membership.workspace().clone();
    match membership.services().fleets().live_set(&workspace).await {
        Ok(fleets) => {
            if wall.fan_in.sync_to(&fleets).await.is_change() {
                Tick::Changed
            } else {
                Tick::Steady
            }
        }
        Err(error) => {
            set_unread(&workspace, &error);
            wall.owed.set_retry = true;
            Tick::Steady
        }
    }
}

/// A set read that did not answer: the wall keeps the set it has.
fn set_unread(workspace: &Uuid7, error: &afd_fleet_lifecycle::Error) {
    let workspace_id = workspace.as_str();
    let Logged { error_code, reason } = error.logged();
    {
        let event = &EVENT_SET_UNREAD;
        tracing::warn!(workspace_id, error_code, reason, event);
    };
}

#[cfg(test)]
mod tests;
