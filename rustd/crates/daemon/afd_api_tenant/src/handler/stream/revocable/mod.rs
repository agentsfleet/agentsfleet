//! How a live stream re-asks its caller's access, and how it ends when the
//! answer turns: `access_revoked`, then nothing.
//!
//! Both streams re-ask when a notice says the account's members changed, or a
//! gap may have hidden one: the fleet stream (`guard.rs`) and the workspace wall
//! (`wall.rs`). Neither asks on a timer. Both ask
//! through one [`Watch`], so they cannot disagree about an unanswered check.
//! The ending is this adapter's, so neither stream carries a "closed" flag a
//! later edit could forget to check. Once a turn says the access is gone, the
//! state that produced it is dropped and no further turn can run.
//!
//! # An unanswered re-check ends the stream
//!
//! A re-check that cannot reach the datastore ends the stream without
//! `access_revoked`, because an outage is not a revocation. The client
//! reconnects, and the route authorizes it again at open, so a removed member
//! never reads on through an outage.

use std::sync::Arc;

use af_core::error_code::ErrorCode;
use af_core::id::Uuid7;
use afd_auth::principal::Principal;
use afd_http::handler::Refusable;
use afd_sse::Frame;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use crate::services::{TenantPlane, WorkspaceOwnership as _};

/// The event a stream ended by an unanswered re-check is logged under.
pub(super) const EVENT_STREAM_UNVERIFIED: &str = "stream_closed_unverified";

/// The event a refused re-check is logged under.
const EVENT_ACCESS_REVOKED: &str = "stream_access_revoked";

/// The `stream` field of a fleet stream's re-check records.
pub(super) const STREAM_FLEET: &str = "fleet";

/// The `stream` field of a workspace wall's re-check records.
pub(super) const STREAM_WALL: &str = "wall";

/// The question a live stream re-asks when a notice says the account's members may have
/// changed, or a gap may have hidden one.
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

impl<D: TenantPlane> Recheck for Membership<D> {
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
    /// No answer: the stream closes without `access_revoked`.
    Unverified,
}

/// A stream's re-check.
pub(super) struct Watch<R> {
    question: R,
    /// Which stream the records name: [`STREAM_FLEET`] or [`STREAM_WALL`].
    stream: &'static str,
}

impl<R: Recheck> Watch<R> {
    /// A watch for `stream` that has asked nothing yet.
    pub(super) const fn new(question: R, stream: &'static str) -> Self {
        Self { question, stream }
    }

    /// The question this watch puts.
    pub(super) const fn question(&self) -> &R {
        &self.question
    }

    /// Asks once.
    pub(super) async fn verdict(&self) -> Verdict {
        match self.question.admitted().await {
            Ok(admitted) => self.on_answer(admitted),
            Err(error) => self.on_silence(error.code(), &error.reason()),
        }
    }

    /// The datastore answered `admitted`.
    fn on_answer(&self, admitted: bool) -> Verdict {
        if admitted {
            return Verdict::Admitted;
        }
        let workspace_id = self.question.workspace().as_str();
        let stream = self.stream;
        tracing::debug!(workspace_id, stream, event = EVENT_ACCESS_REVOKED);
        Verdict::Revoked
    }

    /// The datastore did not answer, for `reason`: logged at `warn`, so a
    /// stream that closed says why.
    fn on_silence(&self, code: ErrorCode, reason: &str) -> Verdict {
        let error_code = code.as_str();
        let workspace_id = self.question.workspace().as_str();
        let stream = self.stream;
        tracing::warn!(
            error_code,
            workspace_id,
            stream,
            reason,
            event = EVENT_STREAM_UNVERIFIED
        );
        Verdict::Unverified
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
mod tests;
