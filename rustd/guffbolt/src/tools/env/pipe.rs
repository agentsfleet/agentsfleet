//! What a process said that its reader has not read yet, for an environment
//! with no channel of its own: its first and last edge.
//!
//! The feeding side never waits on the reader. Output lands in two edges,
//! Codex's split (`core/src/unified_exec/head_tail_buffer.rs`): the head
//! keeps the oldest unread bytes, the tail the newest, and what falls between
//! is counted, never held. Once the head is read, the count of what fell
//! between comes next, then the tail, which becomes the new head. Cuts are
//! moved to character boundaries, so text stays valid UTF-8 on both sides of
//! a gap. The same store sits behind a sandboxing embedder's executor client,
//! so a local process's output reads exactly as a sandboxed one's.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::{Buf as _, Bytes};
use tokio::sync::Notify;

use super::{EDGE_BYTES, Ending, Events, ProcessEvent, Stream};

/// The most bytes before the tail's first character that can belong to one
/// cut off ahead of them: a character is at most four bytes.
const MAX_CONTINUATION: usize = 3;

/// One chunk of output and the stream it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Chunk {
    /// Where it was written.
    stream: Stream,
    /// The bytes.
    data: Bytes,
}

/// What a reader takes next.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Next {
    /// A chunk of output, in the order it was written.
    Output(Chunk),
    /// The bytes that fell between the head and the tail, dropped unread.
    Omitted(u64),
}

/// A process's unread output, cut to its head and tail.
#[derive(Debug)]
struct Unread {
    edge: usize,
    head: VecDeque<Chunk>,
    /// What the head may still take before output goes to the tail; reading
    /// the head never gives any back, only crossing to the tail does.
    head_left: usize,
    tail: VecDeque<Chunk>,
    tail_len: usize,
    /// Bytes dropped unread since the reader last crossed: the tail's front,
    /// and a character the head's last chunk started but never finished.
    gap: u64,
}

impl Unread {
    /// Keeps at most `edge` unread bytes at each end.
    const fn new(edge: usize) -> Self {
        Self {
            edge,
            head: VecDeque::new(),
            head_left: edge,
            tail: VecDeque::new(),
            tail_len: 0,
            gap: 0,
        }
    }

    /// Takes one chunk: what fits the head waits there, the rest in the tail.
    fn push(&mut self, chunk: Chunk) {
        if self.head_left == 0 {
            self.keep(chunk);
        } else if chunk.data.len() < self.head_left {
            self.head_left -= chunk.data.len();
            self.head.push_back(chunk);
        } else {
            let Chunk { stream, mut data } = chunk;
            let rest = data.split_off(boundary(&data, self.head_left));
            self.head_left = 0;
            if !data.is_empty() {
                self.head.push_back(Chunk { stream, data });
            }
            if !rest.is_empty() {
                self.keep(Chunk { stream, data: rest });
            }
        }
    }

    /// The next thing to read: the head's chunks, then the count of what fell
    /// between, then the tail's chunks; `None` once everything is read.
    fn pop(&mut self) -> Option<Next> {
        if self.head.is_empty() {
            let gap = self.cross();
            if gap > 0 {
                return Some(Next::Omitted(gap));
            }
        }
        let mut chunk = self.head.pop_front()?;
        if self.head.is_empty() && self.gap > 0 {
            // A gap follows this chunk, so a character it starts and never
            // finishes would read as noise ahead of the gap: those bytes join
            // it. A gap that opens once the chunk is read is past help.
            let whole = boundary(&chunk.data, chunk.data.len());
            let unfinished = chunk.data.len() - whole;
            if unfinished > 0 {
                chunk.data.truncate(whole);
                self.gap += u64::try_from(unfinished).unwrap_or(u64::MAX);
            }
            if chunk.data.is_empty() {
                return Some(Next::Omitted(self.cross()));
            }
        }
        Some(Next::Output(chunk))
    }

    /// Makes the tail the head, once the head is read, and answers how many
    /// bytes fell between them. The moved tail counts against the new head;
    /// an empty one leaves a whole head for what comes next.
    fn cross(&mut self) -> u64 {
        // Only dropping bytes off the tail's front can cut a character; the
        // head is cut on a boundary. So a tail that never dropped a byte is
        // kept whole, which keeps binary output that merely looks like a
        // character's remains. The remains of a cut character may span the
        // tail's first chunks, so they are counted across them.
        if self.gap > 0 {
            let remains = self
                .tail
                .iter()
                .flat_map(|chunk| chunk.data.iter())
                .take(MAX_CONTINUATION)
                .take_while(|byte| continues_a_character(**byte))
                .count();
            self.drop_front(remains);
        }
        self.head = std::mem::take(&mut self.tail);
        self.head_left = self.edge - std::mem::take(&mut self.tail_len);
        std::mem::take(&mut self.gap)
    }

    /// Appends to the tail, dropping its oldest bytes past the edge.
    fn keep(&mut self, chunk: Chunk) {
        self.tail_len += chunk.data.len();
        self.tail.push_back(chunk);
        self.drop_front(self.tail_len.saturating_sub(self.edge));
    }

    /// Drops `count` bytes from the tail's oldest end, counting them.
    fn drop_front(&mut self, mut count: usize) {
        while let Some(front) = self.tail.front_mut().filter(|_| count > 0) {
            let drop = count.min(front.data.len());
            front.data.advance(drop);
            self.tail_len -= drop;
            self.gap += u64::try_from(drop).unwrap_or(u64::MAX);
            count -= drop;
            if front.data.is_empty() {
                self.tail.pop_front();
            }
        }
    }
}

/// The cut at or before `at` that splits no character.
///
/// A character that `at` would cut in two goes whole to the tail: std's
/// decoder reports it as an incomplete sequence at the end of the prefix.
/// Output that is not text has no characters to split, so it is cut where
/// the cap falls.
fn boundary(data: &[u8], at: usize) -> usize {
    let prefix = data.get(..at).unwrap_or(data);
    let incomplete =
        prefix
            .utf8_chunks()
            .last()
            .map_or(0, |chunk| match std::str::from_utf8(chunk.invalid()) {
                Err(cut) if cut.error_len().is_none() => chunk.invalid().len(),
                _whole_or_not_text => 0,
            });
    prefix.len().saturating_sub(incomplete)
}

/// Whether `byte` continues a multi-byte character rather than starting one:
/// UTF-8 spells every continuation byte `10xxxxxx`.
const fn continues_a_character(byte: u8) -> bool {
    matches!(byte, 0x80..=0xbf)
}

/// What both ends share.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// Woken on every change; one permit is kept when no one waits, so a
    /// change between a reader's look and its wait is never missed.
    changed: Notify,
}

/// What has arrived and not been read.
#[derive(Debug)]
struct State {
    unread: Unread,
    /// How the process ended and whether its output was left open, until
    /// the reader takes it.
    ending: Option<(Ending, bool)>,
    /// Whether anything more can arrive.
    finished: bool,
}

impl Shared {
    /// The state. Every change to it is one push or pop, so a panicking
    /// holder leaves it whole and poisoning is passed over.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The feeding end. Dropping it without [`Feed::end`] finishes the events
/// with no ending.
#[derive(Debug)]
pub struct Feed {
    shared: Arc<Shared>,
}

/// The reading end, as a tool reads any environment's events.
#[derive(Debug)]
pub struct Piped {
    shared: Arc<Shared>,
}

/// A new pair: the feed a process's watcher writes, and the events a call
/// reads.
#[must_use]
pub fn pipe() -> (Feed, Piped) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            unread: Unread::new(EDGE_BYTES),
            ending: None,
            finished: false,
        }),
        changed: Notify::new(),
    });
    let feed = Feed {
        shared: Arc::clone(&shared),
    };
    (feed, Piped { shared })
}

impl Feed {
    /// Hands on one chunk of output.
    pub fn output(&self, stream: Stream, data: Bytes) {
        // Empty frames cannot consume an unbounded number of buffer entries.
        if data.is_empty() {
            return;
        }
        self.shared.state().unread.push(Chunk { stream, data });
        self.shared.changed.notify_one();
    }

    /// Hands on how the process ended and whether its output was still open,
    /// the last thing it says. Dropping the feed then finishes the events
    /// and wakes the reader.
    pub fn end(self, ending: Ending, output_abandoned: bool) {
        self.shared.state().ending = Some((ending, output_abandoned));
    }
}

/// The events are finished only here: a live feed can always say more.
impl Drop for Feed {
    fn drop(&mut self) {
        self.shared.state().finished = true;
        self.shared.changed.notify_one();
    }
}

impl From<Next> for ProcessEvent {
    fn from(next: Next) -> Self {
        match next {
            Next::Output(Chunk { stream, data }) => Self::Output { stream, data },
            Next::Omitted(bytes) => Self::Omitted { bytes },
        }
    }
}

impl Piped {
    /// The next event when one is waiting; `Err(true)` once nothing more
    /// will arrive, `Err(false)` while the process may say more.
    fn try_next(&self) -> Result<ProcessEvent, bool> {
        let mut state = self.shared.state();
        if let Some(next) = state.unread.pop() {
            return Ok(next.into());
        }
        match state.ending.take() {
            Some((ending, output_abandoned)) => Ok(ProcessEvent::Ended {
                ending,
                output_abandoned,
            }),
            None => Err(state.finished),
        }
    }
}

impl Events for Piped {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<ProcessEvent>> + Send + '_>> {
        Box::pin(async move {
            loop {
                match self.try_next() {
                    Ok(event) => return Some(event),
                    Err(true) => return None,
                    Err(false) => self.shared.changed.notified().await,
                }
            }
        })
    }

    fn take_ready(&mut self) -> (Vec<ProcessEvent>, bool) {
        let mut state = self.shared.state();
        let mut events: Vec<ProcessEvent> =
            std::iter::from_fn(|| state.unread.pop().map(ProcessEvent::from)).collect();
        if let Some((ending, output_abandoned)) = state.ending.take() {
            events.push(ProcessEvent::Ended {
                ending,
                output_abandoned,
            });
        }
        (events, state.finished)
    }

    fn is_finished(&self) -> bool {
        self.shared.state().finished
    }

    fn finished(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            while !self.is_finished() {
                self.shared.changed.notified().await;
                // Ready output notifications must yield to deadlines.
                tokio::task::consume_budget().await;
            }
        })
    }

    fn disconnect(&mut self) {}
}
