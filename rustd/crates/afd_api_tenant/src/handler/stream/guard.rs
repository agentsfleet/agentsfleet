//! Re-asking a fleet stream's caller whether they may still read it.
//!
//! The ownership layer decided once, when the stream opened, and a tab stays
//! open for hours. An owner who removes a member expects the member's view to
//! stop, so the question is asked again on a beat and the stream ends with
//! `access_revoked` when the answer turns. The workspace stream asks the same
//! question on its own refresh tick (`wall.rs`), where it also re-reads the
//! fleet set; a single fleet has no set to refresh, so it carries only this.
//!
//! What an unanswered re-check means is the [`Watch`]'s, shared with the wall:
//! budgeted, logged, and capped (`revocable.rs`).

use std::sync::Arc;
use std::time::Duration;

use afd_auth::principal::Principal;
use afd_core::id::Uuid7;
use afd_sse::Frame;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use tokio::time::Instant;

use super::revocable::{Membership, Recheck, STREAM_FLEET, Turn, Verdict, Watch, until_revoked};
use crate::services::Services;

/// How often an open fleet stream re-asks its caller's access.
///
/// The heartbeat's interval: the stream wakes then anyway, so a revocation
/// lands within one heartbeat and adds no wake-up of its own.
const RECHECK_INTERVAL: Duration = afd_sse::HEARTBEAT_INTERVAL;

/// Everything a guarded stream carries between frames.
struct Guard<R> {
    frames: BoxStream<'static, Frame>,
    watch: Watch<R>,
    next_check: Instant,
}

/// `frames`, ended with `access_revoked` once `principal` loses `workspace`.
pub(super) fn guarded<D: Services>(
    frames: BoxStream<'static, Frame>,
    services: Arc<D>,
    principal: Principal,
    workspace: Uuid7,
) -> BoxStream<'static, Frame> {
    watched(frames, Membership::new(services, principal, workspace))
}

/// `frames`, ended with `access_revoked` once `recheck` says no.
fn watched<R: Recheck>(frames: BoxStream<'static, Frame>, recheck: R) -> BoxStream<'static, Frame> {
    let guard = Guard {
        frames,
        watch: Watch::new(recheck, STREAM_FLEET),
        next_check: Instant::now() + RECHECK_INTERVAL,
    };
    until_revoked(guard, step)
}

/// The next frame, or the check that ends the stream.
async fn step<R: Recheck>(mut guard: Guard<R>) -> Option<Turn<Guard<R>>> {
    loop {
        tokio::select! {
            frame = guard.frames.next() => return frame.map(|frame| Turn::Frame(frame, guard)),
            () = tokio::time::sleep_until(guard.next_check) => {
                guard.next_check = Instant::now() + RECHECK_INTERVAL;
                match guard.watch.verdict().await {
                    Verdict::Admitted | Verdict::Deferred => {}
                    Verdict::Revoked => return Some(Turn::Revoked),
                    // An outage is not a revocation: the stream ends with no
                    // frame, and the reconnect is authorized at open.
                    Verdict::Unverified => return None,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "guard/tests.rs"]
mod tests;
