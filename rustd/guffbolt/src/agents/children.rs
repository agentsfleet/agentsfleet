//! The turns a run's children take, polled beside the root loop.
//!
//! A child's turn borrows what the run shares, so it cannot be handed to the
//! runtime as a task of its own; it is a future the root polls whenever its
//! own work waits, on a step, a call or a checkpoint. A loop anywhere in the
//! run queues a turn through the registry, and the root starts it here, so a
//! child's children are polled the same way. Ending the set drops every turn
//! still running, and each call it held open ends interrupted on the way
//! down.

use std::fmt;
use std::pin::Pin;

use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;
use tokio::sync::oneshot;

use super::registry::Turn;
use super::seated::Seated;
use super::{Agents, Me, Pause, Raise, Requests, Seat, Status};
use crate::conversation::Conversation;
use crate::drive::{DriveState, drive_in};
use crate::extension::Plain;
use crate::step::Ending;
use crate::tool::{ErrorCode, ToolOutput};
use crate::transcript::{Message, Transcript};

/// One child's turn, from its first step to its end.
type Running<'s> = Pin<Box<dyn Future<Output = ()> + Send + 's>>;

/// What a child that asked the person a question reads as that call's
/// result: only the root asks the person.
const NO_PERSON: &str = "a child agent cannot ask the person; answer from what you have, or say \
                         what you need in your answer";
/// What a call a stopped turn never finished reads as its result.
const UNFINISHED: &str = "the turn was interrupted before this call finished";

tokio::task_local! {
    /// The children of the run whose root [`Children::beside`] drives, as
    /// that root reads them. A child's turn is polled outside it, so only
    /// the root, and a loop the root runs inside one of its calls, finds it.
    static BESIDE: Agents;
}

/// Done once the run's children are quiet, when loop `me` is the root that
/// [`Children::beside`] drives: every turn ended with its end recorded, as
/// [`Agents::quiet`] waits. At once for a child, which would wait on itself,
/// and for a root driven with no children beside it, which has none to wait
/// for. The pause the wait takes is let go at once.
pub(crate) async fn quiet(me: Me) {
    if me != Me::ROOT {
        return;
    }
    let Ok(agents) = BESIDE.try_with(Agents::clone) else {
        return;
    };
    let _released = agents.quiet().await;
}

/// Every turn a run's children take and have not finished.
pub struct Children<'s> {
    running: FuturesUnordered<Running<'s>>,
    requests: Requests,
    /// Each ask for a pause that waits for the set to be quiet.
    waiting: Vec<oneshot::Sender<Option<Pause>>>,
}

impl<'s> Children<'s> {
    /// No turn yet; `requests` is where the registry asks for one.
    #[must_use]
    pub fn new(requests: Requests) -> Self {
        Self {
            running: FuturesUnordered::new(),
            requests,
            waiting: Vec::new(),
        }
    }

    /// Drives `driven` to its end, starting each turn asked for over the
    /// conversation `raise` gives it and polling every turn whenever `driven`
    /// waits. `driven` goes first, so a loop that never waits is never
    /// slowed by its children. A root `driven` that asks the person a
    /// question ends waiting only once these children are quiet.
    pub async fn beside<R, F>(&mut self, raise: &'s R, driven: F) -> F::Output
    where
        R: Raise,
        F: Future,
    {
        let driven = BESIDE.scope(raise.agents().clone(), driven);
        tokio::pin!(driven);
        loop {
            self.answer_quiet(raise.agents());
            let asked = tokio::select! {
                biased;
                out = &mut driven => return out,
                Some(id) = self.requests.turns.recv() => Some(id),
                Some(()) = self.running.next(), if !self.running.is_empty() => None,
                Some(ask) = self.requests.pauses.recv() => {
                    if ask.waits {
                        self.waiting.push(ask.reply);
                    } else {
                        let pause = raise.agents().try_pause(||
                            self.running.is_empty() && self.requests.is_empty());
                        let _receiver_gone = ask.reply.send(pause);
                    }
                    None
                }
            };
            if let Some(turn) = asked.and_then(|id| turn(raise, id)) {
                self.running.push(turn);
            }
        }
    }

    /// Ends every child with the loop: drops every turn still running, ends
    /// each one queued and never started, and every open child ends
    /// interrupted, keeping what its last finished turn left. Answers every
    /// ask for a pause still waiting with none, and a pause over the quiet
    /// set, when one may be had.
    pub fn end_all(&mut self, agents: &Agents) -> Option<Pause> {
        self.requests.turns.close();
        self.requests.pauses.close();
        self.running.clear();
        while self.requests.turns.try_recv().is_ok() {}
        agents.end_all();
        while let Ok(ask) = self.requests.pauses.try_recv() {
            let _receiver_gone = ask.reply.send(None);
        }
        for reply in self.waiting.drain(..) {
            let _receiver_gone = reply.send(None);
        }
        agents.try_pause(|| self.running.is_empty() && self.requests.is_empty())
    }

    /// Answers every ask waiting for the set to be quiet, once no turn runs
    /// or waits to start: the first takes the pause, and each one after it
    /// is refused while that pause is held.
    fn answer_quiet(&mut self, agents: &Agents) {
        if self.waiting.is_empty() || !self.running.is_empty() || !self.requests.is_empty() {
            return;
        }
        for reply in self.waiting.drain(..) {
            let _receiver_gone = reply.send(agents.try_pause(|| true));
        }
    }
}

impl fmt::Debug for Children<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Children")
            .field("running", &self.running.len())
            .field("queued", &!self.requests.is_empty())
            .field("waiting", &self.waiting.len())
            .finish()
    }
}

/// Child `id`'s turn, when one is queued: its conversation from `raise`,
/// seated, driven to its end, and its end recorded on every path.
fn turn<R: Raise>(raise: &R, id: u64) -> Option<Running<'_>> {
    let agents = raise.agents();
    let Turn {
        depth,
        stop,
        tools,
        transcript,
        state,
    } = agents.begin(id)?;
    let me = Me { id, depth };
    let seat = Seat {
        id,
        depth,
        stop: stop.clone(),
        transcript: Transcript::new(transcript),
    };
    let seated = Seated {
        inner: raise.child(seat),
        agents,
        me,
        tools,
        stop,
    };
    let guard = Guard {
        agents,
        id,
        armed: true,
    };
    Some(Box::pin(run(guard, seated, state)))
}

/// Drives `seated` from `state` to the end of its turn, and records how it
/// ended with what it keeps. The turn answers with what it said alone: a
/// follow-up's wait reads the follow-up's answer, never the earlier ones.
async fn run<C>(guard: Guard<'_>, mut seated: Seated<'_, C>, mut state: DriveState)
where
    C: Conversation<Failure: fmt::Display>,
{
    let me = seated.me;
    state.begin_turn();
    let ending = drive_in(&mut seated, &mut Plain, &mut state, me).await;
    let status = settled(&mut seated, ending, &mut state);
    let transcript = seated.transcript();
    mend(transcript);
    let kept = std::mem::take(transcript).into_messages();
    guard.ended(status, kept, state);
}

/// How a turn that ended as `ending` leaves its child: its answer scrubbed
/// as anything a model wrote; a question it asked answered in its place, so
/// its transcript stays whole for the next turn.
fn settled<C>(
    seated: &mut Seated<'_, C>,
    ending: Ending<C::Failure>,
    state: &mut DriveState,
) -> Status
where
    C: Conversation<Failure: fmt::Display>,
{
    match ending {
        Ending::Answered(mut text) => {
            seated.scrub(&mut text, &mut []);
            Status::Done(text)
        }
        // The alternate form: an error that carries more than its sentence,
        // such as a code or a backtrace, keeps that form for the sentence.
        Ending::Failed(failure) => Status::Failed(format!("{failure:#}")),
        Ending::Stopped => Status::Interrupted,
        Ending::Waiting(waiting) => {
            let refused = ToolOutput::failed(ErrorCode::QUESTION_PENDING, NO_PERSON);
            state.resume(seated.transcript(), refused.text);
            Status::Failed(format!(
                "asked a question only the person can answer: {}",
                waiting.question
            ))
        }
    }
}

/// Answers every call of the transcript's last step that has no result, as
/// a turn stopped mid-step leaves it, so the next turn's request is whole.
fn mend(transcript: &mut Transcript) {
    let messages = transcript.messages();
    let Some((last, calls)) =
        messages
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, message)| match message {
                Message::Assistant { calls, .. } => Some((at, calls)),
                Message::User(_) | Message::ToolResult { .. } => None,
            })
    else {
        return;
    };
    let answered: Vec<&str> = messages
        .iter()
        .skip(last)
        .filter_map(|message| match message {
            Message::ToolResult { call_id, .. } => Some(call_id.as_str()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect();
    let unanswered: Vec<String> = calls
        .iter()
        .filter(|call| !answered.contains(&call.id.as_str()))
        .map(|call| call.id.clone())
        .collect();
    for call_id in unanswered {
        let output = ToolOutput::failed(ErrorCode::INTERRUPTED, UNFINISHED).text;
        transcript.push(Message::ToolResult {
            call_id,
            output,
            image: None,
        });
    }
}

/// One turn's end, recorded on every path: as the turn ended when it ran to
/// its end, and interrupted, keeping what the child had before the turn,
/// when its future is dropped first.
struct Guard<'s> {
    agents: &'s Agents,
    id: u64,
    armed: bool,
}

impl Guard<'_> {
    /// Ends the turn as `status`, keeping `transcript` and `state`.
    fn ended(mut self, status: Status, transcript: Vec<Message>, state: DriveState) {
        self.armed = false;
        self.agents
            .ended(self.id, status, Some((transcript, state)));
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.agents.ended(self.id, Status::Interrupted, None);
        }
    }
}
