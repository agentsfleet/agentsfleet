//! Every child of one run: its id, state, inbox and stop, and the caps.
//!
//! The registry is data behind one short lock; no child's future lives here,
//! so any loop of the run reaches it through the shared state while the root
//! loop polls the futures. A loop asks for a child by reserving it here,
//! which sends the root a [`Start`]; the child's state moves once, from
//! running to its end, and `wait_agent` watches it.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use afr_tools::{Selection, ToolErrorCode};
use serde::{Serialize, Serializer};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

/// How deep a child may be: the root is 0, and a child this deep is offered
/// no nested tool.
pub(crate) const NESTED_DEPTH_MAX: u8 = 2;
/// How many children may run at once.
pub(crate) const CHILDREN_RUNNING_MAX: usize = 4;
/// How many children one run may start.
pub(crate) const CHILDREN_PER_RUN_MAX: u64 = 16;

/// Where a child is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    Running,
    /// Ended on its answer.
    Done(String),
    /// Ended on a failure of its own, with its detail.
    Failed(String),
    /// Ended before its answer: by `interrupt_agent`, or with its parent.
    Interrupted,
}

impl Status {
    /// Whether the child has ended.
    pub(crate) const fn is_final(&self) -> bool {
        !matches!(self, Self::Running)
    }

    /// The status, as a name.
    pub(crate) const fn kind(&self) -> Kind {
        match self {
            Self::Running => Kind::Running,
            Self::Done(_) => Kind::Done,
            Self::Failed(_) => Kind::Failed,
            Self::Interrupted => Kind::Interrupted,
        }
    }
}

/// A status by name, as the model and the log read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Running,
    Done,
    Failed,
    Interrupted,
}

impl Serialize for Kind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl Kind {
    /// The spelling the model and the log read.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

/// A child to start, as the root loop receives it.
pub(crate) struct Start<'run> {
    pub(crate) id: u64,
    /// The tools the child holds: its parent's, narrowed.
    pub(crate) selection: Selection<'run>,
    pub(crate) seat: Seat,
}

/// What a child's loop is given that the root's is not.
pub(crate) struct Seat {
    pub(crate) depth: u8,
    /// The child's first message.
    pub(crate) task: String,
    /// Cancelled to end the child: a descendant of its parent's.
    pub(crate) stop: CancellationToken,
    /// What `send_input` queues for its next turn.
    pub(crate) input: mpsc::UnboundedReceiver<String>,
}

/// One child's state, as `list_agents` reads it.
#[derive(Debug, Serialize)]
pub(crate) struct Listed {
    pub(crate) child_id: u64,
    pub(crate) status: Kind,
    pub(crate) depth: u8,
    pub(crate) calls: u64,
}

/// One child, as the registry keeps it.
struct Child {
    status: watch::Sender<Status>,
    input: mpsc::UnboundedSender<String>,
    stop: CancellationToken,
    depth: u8,
    calls: u64,
}

/// The children, and the run's counts against its caps.
#[derive(Default)]
struct Book {
    next: u64,
    running: usize,
    started: u64,
    children: BTreeMap<u64, Child>,
}

/// Every child of one run.
pub(crate) struct Registry<'run> {
    book: Mutex<Book>,
    requests: mpsc::UnboundedSender<Start<'run>>,
}

impl<'run> Registry<'run> {
    /// No child yet, and where the root loop reads each one to start.
    pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<Start<'run>>) {
        let (requests, asked) = mpsc::unbounded_channel();
        let registry = Self {
            book: Mutex::default(),
            requests,
        };
        (registry, asked)
    }

    /// Reserves a child at `depth` over `task` holding `selection`, ending
    /// with `parent`, and asks the root loop to start it.
    ///
    /// # Errors
    /// As many children run, or were started, as one run may.
    pub(crate) fn start(
        &self,
        depth: u8,
        task: String,
        selection: Selection<'run>,
        parent: &CancellationToken,
    ) -> Result<u64, ToolErrorCode> {
        let mut book = self.book();
        if book.running >= CHILDREN_RUNNING_MAX || book.started >= CHILDREN_PER_RUN_MAX {
            return Err(ToolErrorCode::ChildCapReached);
        }
        book.running += 1;
        book.started += 1;
        book.next += 1;
        let id = book.next;
        let (status, _watched) = watch::channel(Status::Running);
        let (input, inbox) = mpsc::unbounded_channel();
        let stop = parent.child_token();
        book.children.insert(
            id,
            Child {
                status,
                input,
                stop: stop.clone(),
                depth,
                calls: 0,
            },
        );
        drop(book);
        // The root loop holds the receiver for as long as any loop of the run
        // runs, so a request reaches it; one sent after the run ended has no
        // loop left to serve it and goes with the run.
        let _after_the_run = self.requests.send(Start {
            id,
            selection,
            seat: Seat {
                depth,
                task,
                stop,
                input: inbox,
            },
        });
        Ok(id)
    }

    /// Waits for child `id` to end, or for `timeout` to pass, and answers
    /// where it is; `None` for an id no child has. With no timeout it waits
    /// as long as the caller does.
    pub(crate) async fn wait(&self, id: u64, timeout: Option<Duration>) -> Option<Status> {
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
    pub(crate) fn send(&self, id: u64, message: String) -> Option<bool> {
        let book = self.book();
        let child = book.children.get(&id)?;
        let running = !child.status.borrow().is_final();
        Some(running && child.input.send(message).is_ok())
    }

    /// Every child, oldest first.
    pub(crate) fn list(&self) -> Vec<Listed> {
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
    pub(crate) fn interrupt(&self, id: u64) -> Option<Kind> {
        let mut book = self.book();
        book.children
            .contains_key(&id)
            .then(|| book.end(id, Status::Interrupted))
    }

    /// Records that child `id` ended as `status`, and answers the status in
    /// force after: the one given, or the end the child already had.
    pub(crate) fn ended(&self, id: u64, status: Status) -> Kind {
        self.book().end(id, status)
    }

    /// Counts one call child `id` made.
    pub(crate) fn called(&self, id: u64) {
        if let Some(child) = self.book().children.get_mut(&id) {
            child.calls += 1;
        }
    }

    /// How many calls child `id` has made.
    pub(crate) fn calls(&self, id: u64) -> u64 {
        self.book().children.get(&id).map_or(0, |child| child.calls)
    }

    /// The book, whatever a panicking holder left: every change to it is a
    /// few fields set under one lock, so a panic leaves it whole.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Book {
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
        // child that answered leaves no grandchild billing turns or holding
        // a running slot until the root ends.
        child.stop.cancel();
        let kind = status.kind();
        child.status.send_replace(status);
        self.running = self.running.saturating_sub(1);
        kind
    }
}
