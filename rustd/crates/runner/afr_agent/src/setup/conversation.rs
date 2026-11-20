//! A setup run's loop as `guffbolt` drives it: its one tool, its window hook
//! over the draft's context, its live answer and its spend.
//!
//! A setup run takes no input mid-run: the draft's next message is the next
//! run's. Its window is the hook's: the saved summary and the earlier turns
//! it does not cover, ahead of the transcript, compacted when they no longer
//! fit (`super::context`).

use std::future::ready;

use af_context::{Compaction, PortFuture, Summarizer, Summary};
use af_providers::Provider;
use af_secrets::Scrub;
use af_wire::run::RunPayload;
use afr_tools::setup::FleetSetup;
use guffbolt::tools::propose::Propose;
use guffbolt::{
    Call, CallEnd, CallRef, Describe as _, Fitted, Input, Listen, Message, Observe, Request, Step,
    StepEnd, StepOutcome, TextKind, Tool, ToolFuture, ToolOutput, ToolSpec, Transcript, Usage,
    Window,
};
use tokio_util::sync::CancellationToken;
use tracing::Span;

use super::context::Saved;
use crate::engine::{EventSink, Meter};
use crate::events::Live;
use crate::spans;

/// What every model call of the run shares.
pub(super) struct Shared<'a> {
    pub(super) run: &'a RunPayload<'a>,
    pub(super) provider: &'a Provider,
    pub(super) scrub: &'a Scrub,
    /// The answer's text, streamed live.
    pub(super) live: Live<'a>,
    /// Where the run's frames go: a compaction's progress among them.
    pub(super) events: &'a dyn EventSink,
    pub(super) meter: &'a Meter,
    pub(super) stop: &'a CancellationToken,
    /// `propose_setup`, the run's one tool; a clone the run reads what it
    /// kept through.
    pub(super) tool: &'a Propose<FleetSetup>,
}

impl Shared<'_> {
    /// A request for `messages` offering `tools`, as the run sends it.
    fn request<'r>(
        &'r self,
        messages: &'r [Message],
        tools: &'r [ToolSpec<'r>],
        max_output_tokens: Option<u32>,
    ) -> Request<'r> {
        Request {
            model: &self.run.policy.context.model,
            instructions: &self.run.instructions,
            messages,
            tools,
            hosted: &[],
            // A draft's run names its draft here, which keys its prompt cache.
            cache_key: &self.run.event.fleet_id,
            max_output_tokens,
            single_attempt: false,
        }
    }

    /// The most a step may write: the model's own output allowance.
    fn output_tokens(&self) -> u32 {
        self.run
            .policy
            .inference
            .as_ref()
            .map_or(1, |settings| settings.max_output_tokens)
    }

    /// The window the model's context leaves, its trigger at the policy's
    /// threshold; none for a run that names no context size, whose window is
    /// never fitted.
    fn window(&self) -> af_context::Result<Option<af_context::Window>> {
        let policy = &self.run.policy;
        if policy.model_context_tokens == 0 {
            return Ok(None);
        }
        let threshold = policy.context.stage_chunk_threshold;
        let percent = (1_u8..=90)
            .rfind(|percent| f32::from(*percent) / 100.0 <= threshold)
            .map_or(1, u32::from);
        Ok(Some(
            af_context::Window::new(policy.model_context_tokens, self.output_tokens())?
                .with_operating_tokens(policy.context.context_cap_tokens)
                .with_trigger_percent(percent),
        ))
    }

    /// The input `request` costs, as the provider measures it.
    fn measure(&self, request: Request<'_>) -> af_context::Result<u64> {
        self.provider
            .measure(request)
            .map_err(af_context::Error::boundary)
    }
}

/// One setup run's loop.
pub(super) struct SetupLoop<'a> {
    shared: Shared<'a>,
    saved: Saved<'a>,
    transcript: Transcript,
}

impl<'a> SetupLoop<'a> {
    /// The loop over `shared`, its context `saved`, opening with `opening`.
    pub(super) fn new(shared: Shared<'a>, saved: Saved<'a>, opening: Vec<Message>) -> Self {
        Self {
            shared,
            saved,
            transcript: Transcript::new(opening),
        }
    }

    /// The window hook: the summary, the earlier turns and the transcript,
    /// compacting the oldest turns until the rest fits. A turn that still
    /// does not fit once nothing is left to compact is refused here.
    async fn fit(&mut self) -> af_context::Result<Window> {
        let Some(window) = self.shared.window()? else {
            return Ok(Window {
                messages: self.saved.messages(self.transcript.messages())?,
                max_output_tokens: None,
            });
        };
        let specs = [self.shared.tool.spec()];
        let output = self.shared.output_tokens();
        loop {
            let messages = self.saved.messages(self.transcript.messages())?;
            let input =
                self.shared
                    .measure(self.shared.request(&messages, &specs, Some(output)))?;
            let compacting = window.needs_compaction(input);
            let task = if compacting {
                self.saved.compaction(window.output_tokens(), |task| {
                    fits(&self.shared, window, task)
                })?
            } else {
                None
            };
            let Some(task) = task else {
                return Ok(Window {
                    messages,
                    max_output_tokens: Some(window.fit_output(input)?),
                });
            };
            let mut notes = Notes(&self.shared);
            self.saved
                .compact(task, &mut notes, self.shared.events, self.shared.stop)
                .await?;
        }
    }
}

/// Whether `task`'s summary request, with its output, fits `window`.
fn fits(
    shared: &Shared<'_>,
    window: af_context::Window,
    task: &Compaction,
) -> af_context::Result<bool> {
    let messages = [Message::User(task.input()?)];
    let mut request = shared.request(&messages, &[], Some(task.output_tokens));
    request.instructions = af_context::SUMMARY_INSTRUCTIONS;
    request.single_attempt = true;
    let input = shared.measure(request)?;
    Ok(input.saturating_add(u64::from(task.output_tokens)) <= window.safe_tokens())
}

/// The summary call a compaction makes: tools off, nothing streamed live,
/// its spend on the run's meter.
struct Notes<'s, 'a>(&'s Shared<'a>);

impl Summarizer for Notes<'_, '_> {
    fn summarize<'t>(&'t mut self, task: &'t Compaction) -> PortFuture<'t, Summary> {
        Box::pin(async move {
            let shared = self.0;
            let messages = [Message::User(task.input()?)];
            let mut request = shared.request(&messages, &[], Some(task.output_tokens));
            request.instructions = af_context::SUMMARY_INSTRUCTIONS;
            request.single_attempt = true;
            let step = tokio::select! {
                biased;
                () = shared.stop.cancelled() => return Err(af_context::Error::cancelled()),
                step = Step::read(shared.provider.stream(request), shared.meter) => {
                    step.map_err(af_context::Error::boundary)?
                }
            };
            if step.cut
                || !step.finished
                || !step.calls.is_empty()
                || step.usage.prompt() == 0
                || step.usage.output == 0
            {
                return Err(af_context::Error::summary());
            }
            Summary::decode(&shared.scrub.clean(step.text).into_inner())
        })
    }
}

impl guffbolt::Conversation for SetupLoop<'_> {
    type Failure = af_providers::Error;
    type Model = Provider;
    type Observer = Self;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    /// None: the draft's next message is the next run's turn.
    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(Vec::new())
    }

    async fn window(&mut self) -> Fitted<af_providers::Error> {
        match self.fit().await {
            Ok(window) => Fitted::Ready(window),
            Err(_) if self.shared.stop.is_cancelled() => Fitted::Stopped,
            Err(failure) => Fitted::Failed(af_providers::Error::lost(failure)),
        }
    }

    fn model(&self) -> &Provider {
        self.shared.provider
    }

    fn observer(&self) -> &Self {
        self
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        vec![self.shared.tool.spec()]
    }

    fn request<'r>(&'r self, window: &'r Window, tools: &'r [ToolSpec<'r>]) -> Request<'r> {
        self.shared
            .request(&window.messages, tools, window.max_output_tokens)
    }

    /// `propose_setup`, lent nothing; any other name was never offered.
    fn dispatch<'r>(&'r self, call: &'r Call, _call_ref: &'r CallRef) -> ToolFuture<'r> {
        let tool = self.shared.tool;
        if call.name == tool.name() {
            Tool::<()>::call(tool, &call.arguments, ())
        } else {
            Box::pin(ready(ToolOutput::not_offered(&call.name)))
        }
    }

    /// What the model wrote, as the transcript keeps it: scrubbed, so no
    /// secret the run carries reaches the model again.
    fn scrub(&self, text: &mut String, calls: &mut [Call]) {
        let scrub = self.shared.scrub;
        *text = scrub.clean(std::mem::take(text)).into_inner();
        for call in calls {
            call.arguments = scrub
                .clean_json(std::mem::take(&mut call.arguments))
                .into_inner();
        }
    }

    fn stop(&self) -> &CancellationToken {
        self.shared.stop
    }

    fn silent(&self, cut: bool) -> af_providers::Error {
        af_providers::Error::silent(cut)
    }
}

/// The answer streams live as it arrives; what each step spends reaches the
/// run's meter, so a step cut short is still billed.
impl Listen for SetupLoop<'_> {
    fn text(&self, kind: TextKind, text: &str) {
        self.shared.live.text(kind, text);
    }

    fn usage(&self, usage: Usage) {
        self.shared.meter.add(usage);
    }

    fn end(&self) {
        self.shared.live.end();
    }
}

/// Each step is traced as a fleet run's is; each `propose_setup` call runs
/// in its tool span, and its own outcome log is `FleetSetup`'s.
impl Observe<af_providers::Error> for SetupLoop<'_> {
    type Call = Span;

    fn step_started(&self, _number: u64) -> Span {
        spans::chat(&self.shared.run.policy.context.model)
    }

    fn step_ended(&self, end: StepEnd<'_, af_providers::Error>) {
        if let StepOutcome::Read(step) = end.outcome {
            spans::spent(end.span, step.usage);
        }
    }

    fn call_started(&self, call: &Call) -> (Span, Span) {
        let span = spans::execute_tool(&call.name, &call.id);
        (span.clone(), span)
    }

    fn call_ended(&self, kept: Span, end: CallEnd<'_>) {
        spans::failed(&kept, end.error_code());
    }
}
