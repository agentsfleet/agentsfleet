//! How a live stream ends when its caller loses access: `access_revoked`,
//! then nothing.
//!
//! Both streams re-ask the caller's access on a beat and both end the same
//! way. The ending is this adapter's, so neither stream carries a "closed" flag
//! a later edit could forget to check: once a turn says the access is gone, the
//! state that produced it is dropped and no further turn can run.

use afd_sse::Frame;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

/// What one turn of a guarded stream produced.
pub(super) enum Turn<S> {
    /// A frame to send, and the state the next turn starts from.
    Frame(Frame, S),
    /// The caller may no longer read: `access_revoked` goes out last.
    Revoked,
}

/// The frames `step` produces from `state`, ended by `access_revoked` on the
/// turn that reports the access gone, or by `step` running out of frames.
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
