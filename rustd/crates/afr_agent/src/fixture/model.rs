//! The scripted model the loop's suites drive: turns played in order through
//! a lock-free cursor, every request sent back over a channel.

#![expect(
    clippy::expect_used,
    reason = "test fixture: a fixture that cannot be built is a broken test"
)]

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use afd_wire::activity::StreamTextKind;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::ExecutionPolicy;
use afr_providers::{Call, Chunk, Connect, End, Message, Provider, Request, Usage};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;

use super::RECEIVER_HELD;

/// What one request the provider was sent carried.
#[derive(Debug, Clone)]
pub(crate) struct Sent {
    pub(crate) tools: Vec<String>,
    pub(crate) hosted: Vec<&'static str>,
    pub(crate) instructions: String,
    pub(crate) messages: Vec<Message>,
}

/// One scripted turn: the chunks it streams, then the failure it ends on.
#[derive(Debug)]
struct Turn {
    chunks: Vec<Chunk>,
    failure: Option<fn() -> afr_providers::Error>,
}

impl Turn {
    fn replay(&self) -> Vec<afr_providers::Result<Chunk>> {
        let chunks = self.chunks.iter().cloned().map(Ok);
        chunks
            .chain(self.failure.map(|failure| Err(failure())))
            .collect()
    }
}

/// The turns a script plays, in order, read lock-free through a cursor.
#[derive(Debug)]
struct Turns {
    turns: Vec<Turn>,
    next: AtomicUsize,
}

/// A scripted model the test drives and reads back. The loop gets a
/// [`Replay`]; the requests it sends come back over a channel.
#[derive(Debug)]
pub(crate) struct Script {
    replay: Replay,
    received: mpsc::Receiver<Sent>,
    seen: RefCell<Vec<Sent>>,
}

impl Script {
    /// A model whose turns are `turns`, in order.
    pub(crate) fn new(turns: impl IntoIterator<Item = Vec<Chunk>>) -> Self {
        let turns = turns.into_iter().map(|chunks| Turn {
            chunks,
            failure: None,
        });
        Self::playing(turns.collect())
    }

    /// A model whose one turn streams `chunks`, then fails with `failure()`.
    pub(crate) fn failing(chunks: Vec<Chunk>, failure: fn() -> afr_providers::Error) -> Self {
        Self::playing(vec![Turn {
            chunks,
            failure: Some(failure),
        }])
    }

    /// The same model, whose wire takes no image with a call's result.
    pub(crate) fn text_only(mut self) -> Self {
        self.replay.images = false;
        self
    }

    fn playing(turns: Vec<Turn>) -> Self {
        let (sent, received) = mpsc::channel();
        let turns = Arc::new(Turns {
            turns,
            next: AtomicUsize::new(0),
        });
        Self {
            replay: Replay {
                turns,
                sent,
                images: true,
            },
            received,
            seen: RefCell::default(),
        }
    }

    /// The provider the loop drives; every one plays the same turns.
    pub(crate) fn replay(&self) -> Replay {
        self.replay.clone()
    }

    /// Every request sent so far.
    pub(crate) fn sent(&self) -> Vec<Sent> {
        self.seen.borrow_mut().extend(self.received.try_iter());
        self.seen.borrow().clone()
    }
}

/// The provider side of a [`Script`].
#[derive(Debug, Clone)]
pub(crate) struct Replay {
    turns: Arc<Turns>,
    sent: mpsc::Sender<Sent>,
    /// Whether its wire takes an image with a call's result.
    images: bool,
}

impl Connect for Replay {
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> afr_providers::Result<()> {
        Ok(())
    }

    fn connect(&self, _lease: &LeasePayload<'_>) -> afr_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}

/// A model no lease can reach: admitted, then refused at connect with `404`.
#[derive(Debug)]
pub(crate) struct Unreachable;

/// The status [`Unreachable`] refuses with.
pub(crate) const UNREACHABLE_STATUS: u16 = 404;

impl Connect for Unreachable {
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> afr_providers::Result<()> {
        Ok(())
    }

    fn connect(&self, _lease: &LeasePayload<'_>) -> afr_providers::Result<Box<dyn Provider>> {
        Err(afr_providers::Error::refused(UNREACHABLE_STATUS))
    }
}

impl Provider for Replay {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        let sent = Sent {
            tools: request
                .tools
                .iter()
                .map(|spec| spec.name.to_owned())
                .collect(),
            hosted: request.hosted.iter().map(|entry| entry.name()).collect(),
            instructions: request.instructions.to_owned(),
            messages: request.messages.to_vec(),
        };
        self.sent.send(sent).expect(RECEIVER_HELD);
        let index = self.turns.next.fetch_add(1, Ordering::Relaxed);
        let turn = self.turns.turns.get(index).map(Turn::replay);
        futures_util::stream::iter(turn.unwrap_or_default()).boxed()
    }

    fn accepts_images(&self) -> bool {
        self.images
    }
}

/// A call to `name` with `arguments`, under provider id `id`.
pub(crate) fn call(id: &str, name: &str, arguments: serde_json::Value) -> Chunk {
    Chunk::Call(Call {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    })
}

/// Answer text.
pub(crate) fn say(text: &str) -> Chunk {
    Chunk::Text {
        kind: StreamTextKind::Answer,
        text: text.to_owned(),
    }
}

/// What a turn spent.
pub(crate) fn spent(input: u64, cached_input: u64, output: u64) -> Chunk {
    Chunk::Usage(Usage {
        input,
        cached_input,
        output,
    })
}

/// How a turn ended: `cut` when it stopped at its output limit.
pub(crate) fn ended(cut: bool) -> Chunk {
    Chunk::End(End {
        cut,
        ..End::default()
    })
}
