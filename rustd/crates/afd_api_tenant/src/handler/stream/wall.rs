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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use afd_auth::principal::Principal;
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
    /// The caller may no longer read this workspace. The stream must close.
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
    let wall = Wall {
        services,
        workspace,
        principal,
        fan_in,
        next_refresh: Instant::now() + REFRESH_INTERVAL,
        announced: false,
    };
    stream::unfold(wall, step).boxed()
}

/// The next frame, and the wall that produced it.
async fn step<D: Services>(mut wall: Wall<D>) -> Option<(Frame, Wall<D>)> {
    // The set is announced before any activity, so a client knows which tiles
    // to open before the first frame arrives for one of them.
    if !wall.announced {
        wall.announced = true;
        let carried = wall.fan_in.fleets();
        let frame = hello(wall.services.as_ref(), &wall.workspace, carried).await;
        return Some((frame, wall));
    }
    loop {
        if Instant::now() >= wall.next_refresh {
            match refresh(&mut wall).await {
                Tick::Revoked => return None,
                Tick::Changed => {
                    let carried = wall.fan_in.fleets();
                    let frame = hello(wall.services.as_ref(), &wall.workspace, carried).await;
                    return Some((frame, wall));
                }
                Tick::Steady => {}
            }
        }
        // Wake for whichever comes first. Sleeping the whole beat would be
        // fine, but waking on the frame is what keeps latency at the
        // publisher's rather than at the tick's.
        let deadline = wall.next_refresh;
        let arrived = tokio::select! {
            frame = wall.fan_in.next_frame() => Some(frame),
            () = tokio::time::sleep_until(deadline) => None,
        };
        if let Some(frame) = arrived {
            // A gap the server could not carry is exactly the frames that
            // moved the counters, so the next step re-announces the set with
            // where every fleet stands now — the backfill recovers the rows,
            // the fresh `hello` recovers the figures.
            if frame.kind == KIND_CATCHING_UP {
                wall.announced = false;
            }
            return Some((frame, wall));
        }
    }
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
        Ok(Some(_tenant)) => {}
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
