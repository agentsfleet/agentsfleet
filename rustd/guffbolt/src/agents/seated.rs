//! A child's turn as the harness drives it: the embedder's conversation,
//! seated in the run with what is the child's own.
//!
//! The embedder's conversation keeps the transcript, fits the window, makes
//! the request and runs each call; the seat adds the child's inbox, which is
//! its input, scrubbed as the embedder scrubs what a model wrote; its tools,
//! narrowed from its parent's; its stop; and the count of its calls.

use std::future::ready;

use tokio_util::sync::CancellationToken;

use super::{Agents, Me};
use crate::conversation::Conversation;
use crate::drive::Input;
use crate::model::Request;
use crate::tool::{ToolFuture, ToolOutput, ToolSpec};
use crate::transcript::{Call, CallRef, Transcript};
use crate::window::{Fitted, Window};

/// Child `me`'s conversation `inner`, holding `tools` and stopped by `stop`.
pub(super) struct Seated<'s, C> {
    pub(super) inner: C,
    pub(super) agents: &'s Agents,
    pub(super) me: Me,
    pub(super) tools: Vec<String>,
    pub(super) stop: CancellationToken,
}

impl<C> Seated<'_, C> {
    /// Whether the child holds the tool named `name`.
    fn holds(&self, name: &str) -> bool {
        self.tools.iter().any(|held| held == name)
    }
}

impl<C: Conversation> Conversation for Seated<'_, C> {
    type Failure = C::Failure;
    type Model = C::Model;
    type Observer = C::Observer;

    fn transcript(&mut self) -> &mut Transcript {
        self.inner.transcript()
    }

    /// What was sent the child since its last step: its task, a parent's
    /// message or a follow-up, each scrubbed as the embedder scrubs what a
    /// model wrote, since a model wrote it.
    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        let sent = self.agents.take_inbox(self.me.id);
        let input = sent
            .into_iter()
            .map(|mut text| {
                self.inner.scrub(&mut text, &mut []);
                Input::from(text)
            })
            .collect();
        ready(input)
    }

    fn window(&mut self) -> impl Future<Output = Fitted<Self::Failure>> + Send {
        self.inner.window()
    }

    fn model(&self) -> &C::Model {
        self.inner.model()
    }

    fn observer(&self) -> &C::Observer {
        self.inner.observer()
    }

    /// The embedder's tools that the child holds, in the embedder's order.
    fn offered(&self) -> Vec<ToolSpec<'_>> {
        let mut offered = self.inner.offered();
        offered.retain(|spec| self.holds(spec.name));
        offered
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        self.inner.request(window, tools)
    }

    /// Runs `call` through the embedder when the child holds its tool; a
    /// call to any other reads the harness's refusal, as a call to a tool
    /// nobody holds does.
    fn dispatch<'a>(&'a self, call: &'a Call, call_ref: &'a CallRef) -> ToolFuture<'a> {
        if self.holds(&call.name) {
            self.inner.dispatch(call, call_ref)
        } else {
            Box::pin(ready(ToolOutput::not_offered(&call.name)))
        }
    }

    fn scrub(&self, text: &mut String, calls: &mut [Call]) {
        self.inner.scrub(text, calls);
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, cut: bool) -> Self::Failure {
        self.inner.silent(cut)
    }

    async fn called(&mut self) {
        self.agents.called(self.me.id);
        self.inner.called().await;
    }
}
