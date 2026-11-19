//! Every child of one run: its id, state, inbox and stop, and the caps.
//!
//! The registry is data behind one short lock; no child's future lives here,
//! so any loop of the run reaches it while the root polls the futures. A loop
//! asks for a child by reserving it here, which queues the start for the
//! root; the child's state moves once, from running to its end, and a wait
//! watches it.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::start::{Requests, Start};
use super::{Caps, Census, Counted, Kind, Listed, Pause, Status};

/// One child, as the book keeps it.
#[derive(Debug)]
struct Child {
    status: watch::Sender<Status>,
    input: mpsc::UnboundedSender<String>,
    stop: CancellationToken,
    depth: u8,
    calls: u64,
}

/// The children, and the run's counts against its caps.
#[derive(Debug)]
pub(super) struct Book {
    next: u64,
    running: usize,
    started: u64,
    children: BTreeMap<u64, Child>,
}

/// Every child of one run, `S` being the tools a child holds.
#[derive(Debug)]
pub struct Agents<S> {
    book: Mutex<Book>,
    pub(super) caps: Caps,
    /// Where a loop queues each child it starts, for the root.
    pub(super) starts: mpsc::UnboundedSender<Start<S>>,
    /// Where a loop asks the root's poller for a pause.
    pub(super) pauses: mpsc::UnboundedSender<oneshot::Sender<Option<Pause>>>,
    /// Set while a [`Pause`] is held; a start reads it under the book's lock.
    pub(super) gate: Arc<AtomicBool>,
}

impl<S> Agents<S> {
    /// No child yet, under `caps`, and where the root reads each one to
    /// start.
    #[must_use]
    pub fn new(caps: Caps) -> (Self, Requests<S>) {
        Self::restored(caps, Census::default())
    }

    /// The children `census` counted in an earlier run, under `caps`, and
    /// where the root reads each new one to start.
    ///
    /// Every child `census` holds had ended, as one read under a [`Pause`]
    /// has: none of them runs again or takes input, and the ids and counts
    /// go on from where it left them.
    #[must_use]
    pub fn restored(caps: Caps, census: Census) -> (Self, Requests<S>) {
        let mut book = Book {
            next: census.next,
            running: 0,
            started: census.started,
            children: BTreeMap::new(),
        };
        for counted in census.children {
            let (status, _watched) = watch::channel(counted.status);
            let (input, inbox) = mpsc::unbounded_channel();
            drop(inbox);
            let stop = CancellationToken::new();
            stop.cancel();
            let child = Child {
                status,
                input,
                stop,
                depth: counted.depth,
                calls: counted.calls,
            };
            book.children.insert(counted.id, child);
        }
        let (starts, start_requests) = mpsc::unbounded_channel();
        let (pauses, pause_requests) = mpsc::unbounded_channel();
        let agents = Self {
            book: Mutex::new(book),
            caps,
            starts,
            pauses,
            gate: Arc::new(AtomicBool::new(false)),
        };
        let requests = Requests {
            starts: start_requests,
            pauses: pause_requests,
        };
        (agents, requests)
    }

    /// Waits for child `id` to end, or for `timeout` to pass, and answers
    /// where it is; `None` for an id no child has. With no timeout it waits
    /// as long as the caller does.
    pub async fn wait(&self, id: u64, timeout: Option<Duration>) -> Option<Status> {
        let mut watched = self.book().children.get(&id)?.status.subscribe();
        let ended = match timeout {
            Some(limit) => tokio::time::timeout(limit, watched.wait_for(Status::is_final))
                .await
                .ok()
                .and_then(|ended| ended.ok().map(|status| status.clone())),
            None => watched
                .wait_for(Status::is_final)
                .await
                .ok()
                .map(|status| status.clone()),
        };
        Some(ended.unwrap_or_else(|| watched.borrow().clone()))
    }

    /// Queues `message` for child `id`'s next turn; whether it was taken,
    /// which a child that has ended never does. `None` for an id no child
    /// has.
    #[must_use]
    pub fn send(&self, id: u64, message: String) -> Option<bool> {
        let book = self.book();
        let child = book.children.get(&id)?;
        let running = !child.status.borrow().is_final();
        let taken = running && child.input.send(message).is_ok();
        drop(book);
        Some(taken)
    }

    /// Every child, oldest first.
    #[must_use]
    pub fn list(&self) -> Vec<Listed> {
        self.book()
            .children
            .iter()
            .map(|(id, child)| Listed {
                child_id: *id,
                status: child.status.borrow().kind(),
                depth: child.depth,
                calls: child.calls,
            })
            .collect()
    }

    /// Ends child `id` where it is, and answers its status after; one that
    /// had ended keeps the end it had. `None` for an id no child has.
    #[must_use]
    pub fn interrupt(&self, id: u64) -> Option<Kind> {
        let mut book = self.book();
        book.children
            .contains_key(&id)
            .then(|| book.end(id, Status::Interrupted))
    }

    /// Records that child `id` ended as `status`, and answers the status in
    /// force after: the one given, or the end the child already had.
    pub(super) fn ended(&self, id: u64, status: Status) -> Kind {
        self.book().end(id, status)
    }

    /// How many calls child `id` made that answered.
    pub(super) fn calls(&self, id: u64) -> u64 {
        self.book().children.get(&id).map_or(0, |child| child.calls)
    }

    /// What child `id` keeps of its place: the book its calls are counted
    /// in, and `inbox`.
    pub(super) fn tether(&self, id: u64, inbox: mpsc::UnboundedReceiver<String>) -> Tether<'_> {
        Tether {
            book: &self.book,
            id,
            inbox,
        }
    }

    /// The book, whatever a panicking holder left: every change to it is a
    /// few fields set under one lock, so a panic leaves it whole.
    pub(super) fn book(&self) -> MutexGuard<'_, Book> {
        lock(&self.book)
    }
}

impl Book {
    /// Whether one more child may start under `caps`.
    pub(super) const fn admits(&self, caps: Caps) -> bool {
        self.running < caps.running && self.started < caps.started
    }

    /// Books a child at `depth` over `task` holding `tools`, ending with
    /// `parent`: its start, as the root receives it.
    pub(super) fn reserve<S>(
        &mut self,
        depth: u8,
        task: String,
        tools: S,
        parent: &CancellationToken,
    ) -> Start<S> {
        self.running += 1;
        self.started += 1;
        self.next += 1;
        let id = self.next;
        let (status, _watched) = watch::channel(Status::Running);
        let (input, inbox) = mpsc::unbounded_channel();
        let stop = parent.child_token();
        let child = Child {
            status,
            input,
            stop: stop.clone(),
            depth,
            calls: 0,
        };
        self.children.insert(id, child);
        Start {
            id,
            depth,
            task,
            stop,
            tools,
            inbox,
        }
    }

    /// How many children run.
    pub(super) const fn running(&self) -> usize {
        self.running
    }

    /// Every child as it is now, oldest first, with the run's counts.
    pub(super) fn census(&self) -> Census {
        let children = self
            .children
            .iter()
            .map(|(id, child)| Counted {
                id: *id,
                depth: child.depth,
                calls: child.calls,
                status: child.status.borrow().clone(),
            })
            .collect();
        Census {
            next: self.next,
            started: self.started,
            children,
        }
    }

    /// Moves child `id` to `status` when it is still running, and answers
    /// the status in force after.
    fn end(&mut self, id: u64, status: Status) -> Kind {
        let Some(child) = self.children.get(&id) else {
            return status.kind();
        };
        if child.status.borrow().is_final() {
            return child.status.borrow().kind();
        }
        // Every child this one started holds a stop descending from this
        // one's, so cancelling it here ends them however this one ended: a
        // child that answered leaves no grandchild running or holding a
        // running slot until the root ends.
        child.stop.cancel();
        let kind = status.kind();
        child.status.send_replace(status);
        self.running = self.running.saturating_sub(1);
        kind
    }
}

/// What a child's loop keeps of its place in the run: what its parent sends
/// it, and the count of its calls its parent reads.
#[derive(Debug)]
pub struct Tether<'s> {
    book: &'s Mutex<Book>,
    id: u64,
    inbox: mpsc::UnboundedReceiver<String>,
}

impl Tether<'_> {
    /// What the parent sent since the child last looked, oldest first.
    pub fn sent(&mut self) -> Vec<String> {
        let mut sent = Vec::new();
        while let Ok(text) = self.inbox.try_recv() {
            sent.push(text);
        }
        sent
    }

    /// Counts one call the child made that answered.
    pub fn called(&self) {
        if let Some(child) = lock(self.book).children.get_mut(&self.id) {
            child.calls += 1;
        }
    }
}

/// `book`, whatever a panicking holder left.
fn lock(book: &Mutex<Book>) -> MutexGuard<'_, Book> {
    book.lock().unwrap_or_else(PoisonError::into_inner)
}
