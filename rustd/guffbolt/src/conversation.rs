//! The embedder's half of a loop: what only it knows.

use std::future::ready;

use tokio_util::sync::CancellationToken;

use crate::model::{Model, Request};
use crate::observe::Observe;
use crate::tool::{ToolFuture, ToolSpec};
use crate::transcript::{Call, CallRef, Transcript};
use crate::window::{Fitted, Window};

/// One loop's conversation, as the embedder keeps it: where its input comes
/// from, how it fits the model's window, what each call is lent, and what it
/// hides.
///
/// [`crate::drive`] calls these in a fixed order and decides nothing about
/// their content. It makes the model call itself, reads the step, runs its
/// calls and keeps the [`Transcript`]; the embedder holds the transcript and
/// hears the work through its [`Observe`]r. A failure is the embedder's own
/// type, so the loop links no model client, no tool and no secret store. An
/// embedder writes each async method as an `async fn`; its future must be
/// `Send`.
pub trait Conversation: Send + Sync {
    /// Why a model call, or fitting the window, failed.
    type Failure: Send;
    /// The model the loop calls.
    type Model: Model<Error = Self::Failure> + ?Sized;
    /// What hears the loop's work while it happens.
    type Observer: Observe<Self::Failure> + ?Sized;

    /// The conversation the loop keeps.
    fn transcript(&mut self) -> &mut Transcript;

    /// The input that arrived since the last call, each piece as the model
    /// may read it; the loop joins it onto the transcript. Input taken after
    /// an answer follows that answer, which the loop has remembered first.
    fn take_input(&mut self) -> impl Future<Output = Vec<String>> + Send;

    /// What the next model call sends of the transcript, asked once the input
    /// is taken: the whole transcript, or a copy fitted to the model's
    /// window, compacted when the transcript no longer fits. The default
    /// sends the whole transcript.
    fn window(&mut self) -> impl Future<Output = Fitted<Self::Failure>> + Send {
        ready(Fitted::Ready(Window::whole(self.transcript().messages())))
    }

    /// The model the next call goes to.
    fn model(&self) -> &Self::Model;

    /// What hears each model call, each tool call and the loop's end.
    fn observer(&self) -> &Self::Observer;

    /// The tools the model is offered on the next call, in order.
    fn offered(&self) -> Vec<ToolSpec<'_>>;

    /// The next call's request, sending `window` and offering `tools`: the
    /// model, the instructions and the rest are the embedder's.
    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a>;

    /// Runs `call`, which sits at `call_ref`, with whatever this loop lends
    /// it; a tool whose backend acts outside the process is lent `call_ref`
    /// too, so the backend can tell a retry from a new call. A name the loop
    /// holds no tool for answers [`crate::ToolOutput::not_offered`] through
    /// [`crate::Tools::dispatch`].
    fn dispatch<'a>(&'a self, call: &'a Call, call_ref: &'a CallRef) -> ToolFuture<'a>;

    /// Hides from the transcript what must never reach a model again, such
    /// as a secret the model wrote, before a step's text and calls enter it.
    /// The calls already ran with the arguments as the model wrote them.
    fn scrub(&self, _text: &mut String, _calls: &mut [Call]) {}

    /// Cancelled when the loop is to stop. The loop races each model call and
    /// each tool call against it, and ends a call it cuts short
    /// [`crate::CallOutcome::Interrupted`].
    fn stop(&self) -> &CancellationToken;

    /// The failure a blank first step is: a model that answered nothing,
    /// `cut` when its step stopped at the output limit.
    fn silent(&self, cut: bool) -> Self::Failure;

    /// After each call that answered, before the next one runs.
    fn called(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Before the loop ends waiting on a question: done once the run's other
    /// work is quiet, such as every child the run started having ended with
    /// its end recorded ([`crate::agents::Agents::quiet`]), so none is lost
    /// while the run waits. The loop races it against its stop, and its
    /// budget bounds it. Only the root waits for the run's children, since
    /// they include every child loop. The default is done at once.
    fn quiet(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}
