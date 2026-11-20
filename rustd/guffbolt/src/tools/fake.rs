//! Backends that record what a tool asked and answer what a case says, for
//! the suites of the tools built over them.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::tools::Refusal;
use crate::tools::message::{Channel, ChannelFuture, Posted};
use crate::tools::notify::{Notification, Notifier, NotifyFuture, Priority};
use crate::tools::schedule::{Change, Job, Page, ScheduleFuture, Scheduler};
use crate::transcript::CallRef;

/// One call a fake backend was handed, in owned form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// `add`: the call's place, and the job.
    Add(CallRef, String, Option<String>, String, bool),
    /// `list`.
    List,
    /// `update`: the id, and the new expression, zone, message and pause.
    Update(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<bool>,
    ),
    /// `remove`: the id.
    Remove(String),
    /// `run_now`: the id.
    RunNow(String),
    /// `runs`: the id, the limit and the cursor.
    Runs(String, Option<u32>, Option<String>),
    /// `post`: the call's place and the line.
    Post(CallRef, String),
    /// `notify`: the call's place, the message, the title and the priority.
    Notify(CallRef, String, Option<String>, Priority),
}

/// A scheduler, channel and notifier at once, answering `answer` to every
/// call and recording each.
#[derive(Debug)]
pub(crate) struct Fake {
    answer: Result<String, Refusal>,
    posted: Posted,
    asked: Mutex<Vec<Asked>>,
}

impl Fake {
    /// A fake answering `answer`, and taking every line.
    pub(crate) fn answering(answer: Result<String, Refusal>) -> Self {
        Self {
            answer,
            posted: Posted::Delivered,
            asked: Mutex::default(),
        }
    }

    /// The same fake, its channel answering `posted`.
    pub(crate) fn posting(mut self, posted: Posted) -> Self {
        self.posted = posted;
        self
    }

    /// Every call it was handed, in order.
    pub(crate) fn asked(&self) -> Vec<Asked> {
        self.held().clone()
    }

    fn held(&self) -> MutexGuard<'_, Vec<Asked>> {
        self.asked.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records `asked` and answers.
    fn record(&self, asked: Asked) -> ScheduleFuture<'_> {
        self.held().push(asked);
        Box::pin(std::future::ready(self.answer.clone()))
    }
}

/// `text`, owned.
fn owned(text: Option<&str>) -> Option<String> {
    text.map(str::to_owned)
}

impl Scheduler for Fake {
    fn add<'a>(&'a self, call: &'a CallRef, job: Job<'a>) -> ScheduleFuture<'a> {
        self.record(Asked::Add(
            call.clone(),
            job.cron.to_owned(),
            owned(job.timezone),
            job.message.to_owned(),
            job.once,
        ))
    }

    fn list(&self) -> ScheduleFuture<'_> {
        self.record(Asked::List)
    }

    fn update<'a>(
        &'a self,
        _call: &'a CallRef,
        id: &'a str,
        change: Change<'a>,
    ) -> ScheduleFuture<'a> {
        self.record(Asked::Update(
            id.to_owned(),
            owned(change.cron),
            owned(change.timezone),
            owned(change.message),
            change.paused,
        ))
    }

    fn remove<'a>(&'a self, _call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a> {
        self.record(Asked::Remove(id.to_owned()))
    }

    fn run_now<'a>(&'a self, _call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a> {
        self.record(Asked::RunNow(id.to_owned()))
    }

    fn runs<'a>(&'a self, id: &'a str, page: Page<'a>) -> ScheduleFuture<'a> {
        self.record(Asked::Runs(
            id.to_owned(),
            page.limit,
            owned(page.starting_after),
        ))
    }
}

impl Channel for Fake {
    fn post<'a>(&'a self, call: &'a CallRef, text: &'a str) -> ChannelFuture<'a> {
        self.held().push(Asked::Post(call.clone(), text.to_owned()));
        let posted = self.answer.clone().map(|_text| self.posted);
        Box::pin(std::future::ready(posted))
    }
}

impl Notifier for Fake {
    fn notify<'a>(&'a self, call: &'a CallRef, note: Notification<'a>) -> NotifyFuture<'a> {
        self.held().push(Asked::Notify(
            call.clone(),
            note.message.to_owned(),
            owned(note.title),
            note.priority,
        ));
        Box::pin(std::future::ready(self.answer.clone()))
    }
}
