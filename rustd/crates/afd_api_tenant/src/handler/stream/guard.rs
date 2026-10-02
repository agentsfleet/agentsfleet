//! Re-asking a fleet stream's caller whether they may still read it.
//!
//! The ownership layer decided once, when the stream opened, and a tab stays
//! open for hours. An owner who removes a member expects the member's view to
//! stop, so the question is asked again on a beat and the stream ends with
//! `access_revoked` when the answer turns. The workspace stream asks the same
//! question on its own refresh tick (`wall.rs`), where it also re-reads the
//! fleet set; a single fleet has no set to refresh, so it carries only this.
//!
//! A re-check that cannot reach the datastore, or does not answer within
//! [`RECHECK_BUDGET`], keeps the stream, for the reason the wall gives: ending
//! every open stream on a blip turns a short outage into a reconnect storm
//! aimed at the thing that is already down. It is logged, so a revocation that
//! lands late says why.

use std::sync::Arc;
use std::time::Duration;

use afd_auth::principal::Principal;
use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_http::handler::Refusable;
use afd_sse::Frame;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use tokio::time::Instant;

use super::revocable::{Turn, until_revoked};
use crate::services::{Services, WorkspaceOwnership as _};

/// How often an open fleet stream re-asks its caller's access.
///
/// The heartbeat's interval: the stream wakes then anyway, so a revocation
/// lands within one heartbeat and adds no wake-up of its own.
const RECHECK_INTERVAL: Duration = afd_sse::HEARTBEAT_INTERVAL;

/// How long a re-check may hold the stream's frames.
///
/// Frames wait while the check runs. Past this budget the check counts as an
/// outage and asks again on the next beat, so a slow pool delays a revocation
/// by one beat rather than every viewer's frames by the pool's acquire timeout.
const RECHECK_BUDGET: Duration = Duration::from_millis(500);

/// The event a re-check that could not answer is logged under.
const EVENT_RECHECK_DEFERRED: &str = "fleet_stream_recheck_deferred";

/// The reason a re-check past [`RECHECK_BUDGET`] is logged with.
const REASON_BUDGET: &str = "recheck exceeded its budget";

/// The question a guarded stream re-asks on each beat.
pub(super) trait Recheck: Send + Sync + 'static {
    /// What a check that could not answer reports.
    type Error: Refusable + Send;

    /// Whether the caller may still read the workspace.
    fn admitted(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// The workspace the question is about.
    fn workspace(&self) -> &Uuid7;
}

/// The production question: may `principal` still read `workspace`?
struct Membership<D> {
    services: Arc<D>,
    principal: Principal,
    workspace: Uuid7,
}

impl<D: Services> Recheck for Membership<D> {
    type Error = afd_tenant::Error;

    async fn admitted(&self) -> afd_tenant::Result<bool> {
        let access = self
            .services
            .workspaces()
            .authorize(&self.principal, &self.workspace)
            .await?;
        Ok(access.is_some())
    }

    fn workspace(&self) -> &Uuid7 {
        &self.workspace
    }
}

/// Everything a guarded stream carries between frames.
struct Guard<R> {
    frames: BoxStream<'static, Frame>,
    recheck: R,
    next_check: Instant,
}

/// `frames`, ended with `access_revoked` once `principal` loses `workspace`.
pub(super) fn guarded<D: Services>(
    frames: BoxStream<'static, Frame>,
    services: Arc<D>,
    principal: Principal,
    workspace: Uuid7,
) -> BoxStream<'static, Frame> {
    let recheck = Membership {
        services,
        principal,
        workspace,
    };
    watched(frames, recheck)
}

/// `frames`, ended with `access_revoked` once `recheck` says no.
fn watched<R: Recheck>(frames: BoxStream<'static, Frame>, recheck: R) -> BoxStream<'static, Frame> {
    let guard = Guard {
        frames,
        recheck,
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
                if !still_admitted(&guard.recheck).await {
                    return Some(Turn::Revoked);
                }
            }
        }
    }
}

/// Whether the caller may still read the workspace; an outage, or a check
/// past [`RECHECK_BUDGET`], answers yes and is logged.
async fn still_admitted<R: Recheck>(recheck: &R) -> bool {
    match tokio::time::timeout(RECHECK_BUDGET, recheck.admitted()).await {
        Ok(Ok(true)) => true,
        Ok(Ok(false)) => {
            let workspace_id = recheck.workspace().as_str();
            tracing::debug!(workspace_id, event = "fleet_stream_revoked");
            false
        }
        Ok(Err(error)) => {
            deferred(recheck.workspace(), error.code(), &error.reason());
            true
        }
        Err(_elapsed) => {
            deferred(
                recheck.workspace(),
                error_code::INTERNAL_DB_UNAVAILABLE,
                REASON_BUDGET,
            );
            true
        }
    }
}

/// A re-check that could not answer: the stream stays open, and says so.
fn deferred(workspace: &Uuid7, code: ErrorCode, reason: &str) {
    let error_code = code.as_str();
    let workspace_id = workspace.as_str();
    tracing::warn!(
        error_code,
        workspace_id,
        reason,
        event = EVENT_RECHECK_DEFERRED
    );
}

#[cfg(test)]
#[path = "guard/tests.rs"]
mod tests;
