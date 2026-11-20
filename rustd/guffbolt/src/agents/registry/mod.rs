//! Every child of one run: its place, status, inbox, stops and what it keeps
//! between turns, and the caps.
//!
//! The registry is data behind one short lock; no turn's future lives here,
//! so any loop of the run reaches it while the root polls the turns. A loop
//! asks for a turn by queueing the child's id for the root's poller; a
//! child's status moves from running to idle as each turn ends, and a wait
//! watches it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::pause::Ask;
use super::{Caps, Kind, Me, Status};
use crate::drive::DriveState;
use crate::transcript::Message;

mod saved;
mod tools;
mod turns;

/// One child, as the book keeps it.
#[derive(Debug)]
pub(super) struct Child {
    /// The loop that started it: 0 for the root.
    pub(super) parent: u64,
    pub(super) depth: u8,
    /// The tools it holds, by name: its parent's, or fewer.
    pub(super) tools: Vec<String>,
    pub(super) calls: u64,
    pub(super) status: watch::Sender<Status>,
    /// What it reads on its next step, oldest first.
    pub(super) inbox: Vec<String>,
    /// Cancelled when it is closed or the loop ends; its turns' stops and
    /// its children's lives descend from it.
    pub(super) life: CancellationToken,
    /// The stop of the turn queued or running, cancelled to interrupt it: a
    /// turn interrupted before it begins never runs.
    pub(super) turn: Option<CancellationToken>,
    /// Whether the queued turn began.
    pub(super) begun: bool,
    /// Whether a follow-up the running turn has not read owes it another
    /// turn, should this one end without reading it.
    pub(super) owed: bool,
    /// The most steps any of its turns began, a turn dropped before its end
    /// included: the next turn's calls are numbered after them, so no
    /// [`crate::CallRef`] repeats.
    pub(super) steps: u64,
    /// Its transcript as its last turn left it.
    pub(super) transcript: Vec<Message>,
    /// Its loop's progress as its last turn left it.
    pub(super) state: DriveState,
    /// Whether it closes once its running turn ends.
    pub(super) closing: bool,
}

/// The children, and the run's counts against its caps.
#[derive(Debug, Default)]
pub(super) struct Book {
    /// The id the last child took.
    pub(super) next: u64,
    /// How many children are open: running or idle.
    pub(super) open: usize,
    /// How many children the run started.
    pub(super) started: u64,
    /// How many turns run, or wait to start.
    pub(super) running: usize,
    pub(super) children: BTreeMap<u64, Child>,
}

/// What every loop of a run shares of its children.
pub(super) struct Shared {
    book: Mutex<Book>,
    pub(super) caps: Caps,
    /// Where a loop queues a child's turn, for the root's poller.
    turns: mpsc::UnboundedSender<u64>,
    /// Where a loop asks the root's poller for a pause.
    pub(super) pauses: mpsc::UnboundedSender<Ask>,
    /// Set while a [`super::Pause`] is held; a start reads it under the
    /// book's lock.
    pub(super) gate: Arc<AtomicBool>,
    /// Every child's life descends from it, so cancelling it ends them all.
    pub(super) root: CancellationToken,
}

/// Every child of one run, behind a handle each of the seven tools holds a
/// copy of.
#[derive(Clone)]
pub struct Agents(pub(super) Arc<Shared>);

impl fmt::Debug for Agents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let book = self.book();
        f.debug_struct("Agents")
            .field("caps", &self.0.caps)
            .field("open", &book.open)
            .field("running", &book.running)
            .field("started", &book.started)
            .finish_non_exhaustive()
    }
}

/// What the root's [`super::Children`] reads: each turn asked for, and each
/// ask for a pause.
#[derive(Debug)]
pub struct Requests {
    pub(super) turns: mpsc::UnboundedReceiver<u64>,
    pub(super) pauses: mpsc::UnboundedReceiver<Ask>,
}

impl Requests {
    /// Whether no turn waits to start.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}

/// One child, as `list_agents` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listed {
    /// Its id.
    pub agent_id: u64,
    /// Where it is.
    pub status: Kind,
    /// How many loops it is inside.
    pub depth: u8,
    /// How many calls it made that answered.
    pub calls: u64,
}

/// What a turn starts from, as the poller reads it.
#[derive(Debug)]
pub(super) struct Turn {
    pub(super) depth: u8,
    pub(super) stop: CancellationToken,
    pub(super) tools: Vec<String>,
    pub(super) transcript: Vec<Message>,
    pub(super) state: DriveState,
}

impl Agents {
    /// No child yet, under `caps`, and where the root reads each turn to
    /// start.
    #[must_use]
    pub fn new(caps: Caps) -> (Self, Requests) {
        Self::restored(caps, &DriveState::default())
    }

    /// Reserves a child of `parent` at its next depth, holding `tools`,
    /// opening with `opening` and given `message` as its first task, and
    /// queues its first turn.
    ///
    /// # Errors
    /// As many children are open, or were started, as the run's caps allow,
    /// or the children are paused.
    pub(super) fn reserve(
        &self,
        parent: Me,
        tools: Vec<String>,
        opening: Vec<Message>,
        message: String,
    ) -> Result<u64, super::Refused> {
        let caps = self.0.caps;
        let mut book = self.book();
        if self.paused() || book.open >= caps.open || book.started >= caps.started {
            return Err(super::Refused::Capped);
        }
        let life = book
            .children
            .get(&parent.id)
            .map_or(&self.0.root, |parent| &parent.life)
            .child_token();
        book.next += 1;
        book.started += 1;
        book.open += 1;
        book.running += 1;
        let id = book.next;
        let (status, _watched) = watch::channel(Status::Running);
        let child = Child {
            parent: parent.id,
            depth: parent.depth.saturating_add(1),
            tools,
            calls: 0,
            status,
            inbox: vec![message],
            turn: Some(life.child_token()),
            life,
            begun: false,
            owed: false,
            steps: 0,
            transcript: opening,
            state: DriveState::default(),
            closing: false,
        };
        book.children.insert(id, child);
        // Queued under the same lock, so a pause granted after it sees the
        // turn waiting. The root holds the receiver for as long as any loop
        // of the run runs; a turn queued after the run ended has no loop left
        // to serve it and goes with the run.
        let _after_the_run = self.0.turns.send(id);
        drop(book);
        Ok(id)
    }

    /// Ends every child with the loop: each life is cancelled, so each
    /// running turn stops. A running child ends interrupted, keeping its
    /// transcript as its last finished turn left it, or closed when it was
    /// closing; an idle child keeps the status its last turn left.
    pub(super) fn end_all(&self) {
        self.0.root.cancel();
        let mut book = self.book();
        let mut closing = Vec::new();
        for (id, child) in &mut book.children {
            child.turn = None;
            child.begun = false;
            child.owed = false;
            child.inbox.clear();
            if *child.status.borrow() == Status::Running {
                child.status.send_replace(Status::Interrupted);
                if child.closing {
                    closing.push(*id);
                }
            }
        }
        book.running = 0;
        for id in closing {
            book.close(id);
        }
    }

    /// The book, whatever a panicking holder left: every change to it is a
    /// few fields set under one lock, so a panic leaves it whole.
    pub(super) fn book(&self) -> MutexGuard<'_, Book> {
        self.0.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a pause holds the children.
    fn paused(&self) -> bool {
        self.0.gate.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Book {
    /// Closes child `id`: its life is cancelled, which stops its running
    /// turn and its children's, and an idle child ends closed here, keeping
    /// nothing; a running one is marked to close as its turn ends.
    fn close(&mut self, id: u64) {
        let Some(child) = self.children.get_mut(&id) else {
            return;
        };
        child.life.cancel();
        let status = child.status.borrow().clone();
        match status {
            Status::Closed => {}
            Status::Running => child.closing = true,
            Status::Done(_) | Status::Failed(_) | Status::Interrupted => {
                child.status.send_replace(Status::Closed);
                child.transcript = Vec::new();
                child.state = DriveState::default();
                child.inbox.clear();
                self.open = self.open.saturating_sub(1);
            }
        }
    }
}
