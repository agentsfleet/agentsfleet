//! How a live stream re-asks its caller's access, and how it ends when the
//! answer turns: `access_revoked`, then nothing.
//!
//! Both streams re-ask on a beat: the fleet stream on its heartbeat
//! (`guard.rs`), the workspace wall on its refresh tick (`wall.rs`). Both ask
//! through one [`Watch`], so they cannot disagree about an unanswered check.
//! The ending is this adapter's, so neither stream carries a "closed" flag a
//! later edit could forget to check. Once a turn says the access is gone, the
//! state that produced it is dropped and no further turn can run.
//!
//! # An unanswered re-check keeps the stream, up to a ceiling
//!
//! A re-check that cannot reach the datastore, or overruns [`RECHECK_BUDGET`],
//! keeps the stream. Ending every stream on a blip aims a reconnect storm at
//! the thing that is already down. [`MAX_DEFERRED_RECHECKS`] in a row end the
//! stream without `access_revoked`, because an outage is not a revocation. The
//! client reconnects, and the route authorizes it again at open.

use std::sync::Arc;
use std::time::Duration;

use afd_auth::principal::Principal;
use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_http::handler::Refusable;
use afd_sse::Frame;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use crate::services::{Services, WorkspaceOwnership as _};

/// How long a re-check may hold the stream's frames.
///
/// Frames wait while the check runs. Past this budget the check counts as
/// unanswered and asks again on the next beat, so a slow pool delays a
/// revocation by one beat rather than every viewer's frames by the pool's
/// acquire timeout.
pub(super) const RECHECK_BUDGET: Duration = Duration::from_millis(500);

/// How many re-checks in a row may go unanswered before the stream ends.
///
/// A minute of the fleet stream's 15-second beat, 40 seconds of the wall's
/// tick. Without a ceiling, a removed member reads on for as long as the
/// datastore stays slow.
pub(super) const MAX_DEFERRED_RECHECKS: u32 = 4;

/// The event a re-check that could not answer is logged under.
pub(super) const EVENT_RECHECK_DEFERRED: &str = "stream_recheck_deferred";

/// The event a stream ended by [`MAX_DEFERRED_RECHECKS`] is logged under.
pub(super) const EVENT_STREAM_UNVERIFIED: &str = "stream_closed_unverified";

/// The event a refused re-check is logged under.
const EVENT_ACCESS_REVOKED: &str = "stream_access_revoked";

/// The `stream` field of a fleet stream's re-check records.
pub(super) const STREAM_FLEET: &str = "fleet";

/// The `stream` field of a workspace wall's re-check records.
pub(super) const STREAM_WALL: &str = "wall";

/// The reason a re-check past [`RECHECK_BUDGET`] is logged with.
pub(super) const REASON_BUDGET: &str = "recheck exceeded its budget";

/// The question a live stream re-asks on each beat.
pub(super) trait Recheck: Send + Sync + 'static {
    /// What a check that could not answer reports.
    type Error: Refusable + Send;

    /// Whether the caller may still read the workspace.
    fn admitted(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// The workspace the question is about.
    fn workspace(&self) -> &Uuid7;
}

/// The production question: may `principal` still read `workspace`?
pub(super) struct Membership<D> {
    services: Arc<D>,
    principal: Principal,
    workspace: Uuid7,
}

impl<D> Membership<D> {
    /// The question for `principal` in `workspace`, put to `services`.
    pub(super) const fn new(services: Arc<D>, principal: Principal, workspace: Uuid7) -> Self {
        Self {
            services,
            principal,
            workspace,
        }
    }

    /// The services the question is put to; the wall reads its set there too.
    pub(super) fn services(&self) -> &D {
        &self.services
    }
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

/// What one re-check concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The caller may still read the workspace.
    Admitted,
    /// The caller may not: the stream sends `access_revoked` and closes.
    Revoked,
    /// No answer this beat: the stream goes on and asks again on the next.
    Deferred,
    /// No answer for [`MAX_DEFERRED_RECHECKS`] beats running: the stream
    /// closes without `access_revoked`.
    Unverified,
}

/// A stream's re-check, and how many beats running it has gone unanswered.
pub(super) struct Watch<R> {
    question: R,
    unanswered: u32,
    /// Which stream the records name: [`STREAM_FLEET`] or [`STREAM_WALL`].
    stream: &'static str,
}

impl<R: Recheck> Watch<R> {
    /// A watch for `stream` that has asked nothing yet.
    pub(super) const fn new(question: R, stream: &'static str) -> Self {
        Self {
            question,
            unanswered: 0,
            stream,
        }
    }

    /// The question this watch puts.
    pub(super) const fn question(&self) -> &R {
        &self.question
    }

    /// Asks once, within [`RECHECK_BUDGET`]. An answer of either kind
    /// starts the unanswered count again.
    pub(super) async fn verdict(&mut self) -> Verdict {
        let budgeted = tokio::time::timeout(RECHECK_BUDGET, self.question.admitted()).await;
        let (code, reason) = match budgeted {
            Ok(Ok(admitted)) => return self.on_answer(admitted),
            Ok(Err(error)) => (error.code(), error.reason()),
            Err(_elapsed) => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                REASON_BUDGET.to_owned(),
            ),
        };
        self.on_silence(code, &reason)
    }

    /// The datastore answered `admitted`.
    fn on_answer(&mut self, admitted: bool) -> Verdict {
        self.unanswered = 0;
        if admitted {
            return Verdict::Admitted;
        }
        let workspace_id = self.question.workspace().as_str();
        let stream = self.stream;
        tracing::debug!(workspace_id, stream, event = EVENT_ACCESS_REVOKED);
        Verdict::Revoked
    }

    /// The datastore did not answer, for `reason`: logged at `warn` either
    /// way, so a revocation that lands late, or a stream that closed, says why.
    fn on_silence(&mut self, code: ErrorCode, reason: &str) -> Verdict {
        self.unanswered = self.unanswered.saturating_add(1);
        let (verdict, event) = if self.unanswered < MAX_DEFERRED_RECHECKS {
            (Verdict::Deferred, EVENT_RECHECK_DEFERRED)
        } else {
            (Verdict::Unverified, EVENT_STREAM_UNVERIFIED)
        };
        let error_code = code.as_str();
        let workspace_id = self.question.workspace().as_str();
        let stream = self.stream;
        tracing::warn!(error_code, workspace_id, stream, reason, event);
        verdict
    }
}

/// What one turn of a guarded stream produced.
pub(super) enum Turn<S> {
    /// A frame to send, and the state the next turn starts from.
    Frame(Frame, S),
    /// The caller may no longer read: `access_revoked` goes out last.
    Revoked,
}

/// The frames `step` produces from `state`. `access_revoked` ends them on the
/// turn that reports the access gone. A `None` turn ends them with no frame:
/// the frames ran out, or the re-check went [`Verdict::Unverified`].
pub(super) fn until_revoked<S, F, Fut>(state: S, mut step: F) -> BoxStream<'static, Frame>
where
    S: Send + 'static,
    F: FnMut(S) -> Fut + Send + 'static,
    Fut: Future<Output = Option<Turn<S>>> + Send + 'static,
{
    stream::unfold(Some(state), move |state| {
        let turn = state.map(&mut step);
        async move {
            match turn?.await? {
                Turn::Frame(frame, next) => Some((frame, Some(next))),
                Turn::Revoked => Some((Frame::access_revoked(), None)),
            }
        }
    })
    .boxed()
}

#[cfg(test)]
#[path = "revocable/tests.rs"]
mod tests;
