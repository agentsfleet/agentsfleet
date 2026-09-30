//! Re-asking a fleet stream's caller whether they may still read it.
//!
//! The ownership layer decided once, when the stream opened, and a tab stays
//! open for hours. An owner who removes a member expects the member's view to
//! stop, so the question is asked again on a beat and the stream ends with
//! `access_revoked` when the answer turns. The workspace stream asks the same
//! question on its own refresh tick (`wall.rs`), where it also re-reads the
//! fleet set; a single fleet has no set to refresh, so it carries only this.
//!
//! A re-check that cannot reach the datastore keeps the stream, for the reason
//! the wall gives: ending every open stream on a blip turns a short outage into
//! a reconnect storm aimed at the thing that is already down.

use std::sync::Arc;
use std::time::Duration;

use afd_auth::principal::Principal;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_sse::Frame;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use tokio::time::Instant;

use crate::services::{Services, WorkspaceOwnership as _};

/// How often an open fleet stream re-asks its caller's access.
///
/// The heartbeat's interval: the stream wakes then anyway, so a revocation
/// lands within one heartbeat and adds no wake-up of its own.
const RECHECK_INTERVAL: Duration = afd_sse::HEARTBEAT_INTERVAL;

/// Everything a guarded stream carries between frames.
struct Guard<D> {
    frames: BoxStream<'static, Frame>,
    services: Arc<D>,
    principal: Principal,
    workspace: Uuid7,
    next_check: Instant,
    /// Whether `access_revoked` has been sent, after which nothing else is.
    closed: bool,
}

/// `frames`, ended with `access_revoked` once `principal` loses `workspace`.
pub(super) fn guarded<D: Services>(
    frames: BoxStream<'static, Frame>,
    services: Arc<D>,
    principal: Principal,
    workspace: Uuid7,
) -> BoxStream<'static, Frame> {
    let guard = Guard {
        frames,
        services,
        principal,
        workspace,
        next_check: Instant::now() + RECHECK_INTERVAL,
        closed: false,
    };
    stream::unfold(guard, step).boxed()
}

/// The next frame, or the check that ends the stream.
async fn step<D: Services>(mut guard: Guard<D>) -> Option<(Frame, Guard<D>)> {
    if guard.closed {
        return None;
    }
    loop {
        tokio::select! {
            frame = guard.frames.next() => return frame.map(|frame| (frame, guard)),
            () = tokio::time::sleep_until(guard.next_check) => {
                guard.next_check = Instant::now() + RECHECK_INTERVAL;
                if still_admitted(&mut guard).await {
                    continue;
                }
                guard.closed = true;
                let refused = error_code::AUTH_FORBIDDEN.as_str();
                return Some((Frame::access_revoked(refused), guard));
            }
        }
    }
}

/// Whether the caller may still read the workspace; an outage answers yes.
///
/// `&mut` rather than `&`, as the wall's refresh takes it: the boxed stream
/// inside is `Send` but not `Sync`, so a shared borrow held across the read
/// would make the stream's future unsendable.
async fn still_admitted<D: Services>(guard: &mut Guard<D>) -> bool {
    let answer = guard
        .services
        .workspaces()
        .authorize(&guard.principal, &guard.workspace)
        .await;
    if let Ok(None) = answer {
        let workspace_id = guard.workspace.as_str();
        tracing::debug!(workspace_id, event = "fleet_stream_revoked");
        return false;
    }
    true
}
