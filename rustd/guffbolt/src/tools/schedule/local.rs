//! The scheduler of a standalone harness: jobs kept in one file under a
//! directory the embedder names, so they survive a restart.
//!
//! It keeps no clock. The embedder calls [`LocalScheduler::due`] when it
//! likes, and gets each job whose moment has come, once: a recurring job
//! moves on to its next moment after `now`, so a scheduler that was down for
//! a week fires each job once, not once per missed moment; a one-off job is
//! retired. Every change is written through before the call answers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::moment::next_after;
use super::{Change, Job, Page, ScheduleFuture, Scheduler};
use crate::tools::{Refusal, code};
use crate::transcript::CallRef;

/// The file the jobs are kept in, under the embedder's directory.
const FILE: &str = "schedules.json";
/// Where a write lands before it replaces the file, so a crash mid-write
/// leaves the last whole one.
const SCRATCH: &str = "schedules.json.partial";
/// The zone an expression is read in when a job names none.
const UTC: &str = "UTC";
/// The most fires one job remembers, newest kept.
const RUNS_KEPT: usize = 100;
/// How many fires a runs page holds when the call names no limit.
const RUNS_PAGE_DEFAULT: u32 = 50;
/// What a runs cursor that is no fired moment reads back, before it.
const BAD_CURSOR: &str = "starting_after must be a next_cursor runs answered, not";
/// What an id no job has reads back, before the id.
const UNKNOWN: &str = "no schedule has the id";

/// A job whose moment came, as [`LocalScheduler::due`] hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// The job's id, as `cron_list` names it.
    pub id: String,
    /// What the run is asked to do.
    pub message: String,
    /// The moment it was due.
    pub at: SystemTime,
}

/// One kept job, as the file holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    id: String,
    cron: String,
    timezone: String,
    message: String,
    once: bool,
    paused: bool,
    next: Timestamp,
    /// Fired by `cron_run`, ahead of its moment.
    fire_now: bool,
    /// The call that added it, so a retry of that call adds nothing.
    added_by: CallRef,
    /// The moments it fired, oldest first.
    fired: Vec<Timestamp>,
}

impl Kept {
    /// What the model reads of it.
    fn view(&self) -> Value {
        json!({
            "schedule_id": self.id,
            "cron": self.cron,
            "timezone": self.timezone,
            "message": self.message,
            "once": self.once,
            "paused": self.paused,
            "next_run_at": self.next.to_string(),
        })
    }
}

/// Everything the file holds.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Book {
    /// The number the next job's id takes.
    next_id: u64,
    jobs: Vec<Kept>,
}

/// Jobs kept in a file, fired when the embedder asks what is due.
#[derive(Debug)]
pub struct LocalScheduler {
    directory: PathBuf,
    book: Mutex<Book>,
}

impl LocalScheduler {
    /// The scheduler whose jobs are kept under `directory`, holding whatever
    /// an earlier one kept there.
    ///
    /// # Errors
    /// The directory cannot be made, or the file is there and cannot be read,
    /// or does not hold jobs.
    pub fn open(directory: impl AsRef<Path>) -> io::Result<Self> {
        let directory = directory.as_ref().to_owned();
        fs::create_dir_all(&directory)?;
        let book = match fs::read(directory.join(FILE)) {
            Ok(held) => serde_json::from_slice(&held).map_err(io::Error::other)?,
            Err(missing) if missing.kind() == io::ErrorKind::NotFound => Book::default(),
            Err(unreadable) => return Err(unreadable),
        };
        Ok(Self {
            directory,
            book: Mutex::new(book),
        })
    }

    /// Every job whose moment is at or before `now`, once: each recurring one
    /// moves to its next moment after `now`, and each one-off is retired.
    ///
    /// # Errors
    /// `now` is outside the range a moment is kept in, or the file could not
    /// be written; then nothing is handed over and nothing moves.
    pub fn due(&self, now: SystemTime) -> io::Result<Vec<Due>> {
        let now = Timestamp::try_from(now).map_err(io::Error::other)?;
        let mut book = self.held();
        let mut kept = book.jobs.clone();
        let mut due = Vec::new();
        for job in &mut kept {
            let reached = job.fire_now || job.next <= now;
            if job.paused || !reached {
                continue;
            }
            let at = if job.fire_now { now } else { job.next };
            due.push(Due {
                id: job.id.clone(),
                message: job.message.clone(),
                at: SystemTime::try_from(at).map_err(io::Error::other)?,
            });
            job.fire_now = false;
            job.fired.push(at);
            let excess = job.fired.len().saturating_sub(RUNS_KEPT);
            job.fired.drain(..excess);
            if !job.once {
                job.next = next_after(&job.cron, &job.timezone, now).map_err(refused_io)?;
            }
        }
        if due.is_empty() {
            return Ok(due);
        }
        kept.retain(|job| !job.once || job.fired.is_empty());
        let changed = Book {
            next_id: book.next_id,
            jobs: kept,
        };
        self.write(&changed)?;
        *book = changed;
        Ok(due)
    }

    /// The jobs. Every change is one whole replacement, so a panicking
    /// holder leaves them whole.
    fn held(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes `book` to the file, through a scratch file it then replaces.
    fn write(&self, book: &Book) -> io::Result<()> {
        let written = serde_json::to_vec_pretty(book).map_err(io::Error::other)?;
        let scratch = self.directory.join(SCRATCH);
        fs::write(&scratch, written)?;
        fs::rename(scratch, self.directory.join(FILE))
    }

    /// Applies `change` to a copy of the book, writes it, and keeps it; the
    /// answer is what `change` answered.
    fn changed(
        &self,
        change: impl FnOnce(&mut Book) -> Result<String, Refusal>,
    ) -> Result<String, Refusal> {
        let mut book = self.held();
        let mut copy = Book {
            next_id: book.next_id,
            jobs: book.jobs.clone(),
        };
        let answer = change(&mut copy)?;
        self.write(&copy)
            .map_err(|unsaved| Refusal::new(code::SCHEDULE_UNSAVED, unsaved.to_string()))?;
        *book = copy;
        Ok(answer)
    }

    /// What `read` answers of the book as it stands.
    fn read(&self, read: impl FnOnce(&Book) -> Result<String, Refusal>) -> Result<String, Refusal> {
        read(&self.held())
    }
}

impl Scheduler for LocalScheduler {
    fn add<'a>(&'a self, call: &'a CallRef, job: Job<'a>) -> ScheduleFuture<'a> {
        Box::pin(std::future::ready(self.changed(|book| {
            if let Some(added) = book.jobs.iter().find(|kept| kept.added_by == *call) {
                return Ok(added.view().to_string());
            }
            let timezone = job.timezone.unwrap_or(UTC).to_owned();
            let next = next_after(job.cron, &timezone, Timestamp::now())?;
            book.next_id += 1;
            let kept = Kept {
                id: format!("job-{}", book.next_id),
                cron: job.cron.to_owned(),
                timezone,
                message: job.message.to_owned(),
                once: job.once,
                paused: false,
                next,
                fire_now: false,
                added_by: call.clone(),
                fired: Vec::new(),
            };
            let answer = kept.view().to_string();
            book.jobs.push(kept);
            Ok(answer)
        })))
    }

    fn list(&self) -> ScheduleFuture<'_> {
        Box::pin(std::future::ready(self.read(|book| {
            let views: Vec<Value> = book.jobs.iter().map(Kept::view).collect();
            Ok(json!({ "schedules": views }).to_string())
        })))
    }

    fn update<'a>(
        &'a self,
        _call: &'a CallRef,
        id: &'a str,
        change: Change<'a>,
    ) -> ScheduleFuture<'a> {
        Box::pin(std::future::ready(self.changed(|book| {
            let job = find(book, id)?;
            let cron = change.cron.unwrap_or(&job.cron).to_owned();
            let timezone = change.timezone.unwrap_or(&job.timezone).to_owned();
            job.next = next_after(&cron, &timezone, Timestamp::now())?;
            job.cron = cron;
            job.timezone = timezone;
            if let Some(message) = change.message {
                message.clone_into(&mut job.message);
            }
            job.paused = change.paused.unwrap_or(job.paused);
            Ok(job.view().to_string())
        })))
    }

    fn remove<'a>(&'a self, _call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a> {
        Box::pin(std::future::ready(self.changed(|book| {
            find(book, id)?;
            book.jobs.retain(|job| job.id != id);
            Ok(String::new())
        })))
    }

    fn run_now<'a>(&'a self, _call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a> {
        Box::pin(std::future::ready(self.changed(|book| {
            let job = find(book, id)?;
            job.fire_now = true;
            Ok(job.view().to_string())
        })))
    }

    fn runs<'a>(&'a self, id: &'a str, page: Page<'a>) -> ScheduleFuture<'a> {
        Box::pin(std::future::ready(self.read(|book| {
            let job = (book.jobs.iter().find(|job| job.id == id)).ok_or_else(|| unknown(id))?;
            let before = match page.starting_after {
                Some(cursor) => cursor.parse::<Timestamp>().map_err(|_unreadable| {
                    Refusal::new(code::INVALID_ARGUMENTS, format!("{BAD_CURSOR} {cursor}"))
                })?,
                None => Timestamp::MAX,
            };
            let limit = page.limit.unwrap_or(RUNS_PAGE_DEFAULT);
            let limit = usize::try_from(limit).unwrap_or(usize::MAX);
            let fired: Vec<String> = (job.fired.iter().rev())
                .filter(|fired| **fired < before)
                .take(limit)
                .map(ToString::to_string)
                .collect();
            let next_cursor = (fired.len() == limit).then(|| fired.last().cloned());
            Ok(json!({ "runs": fired, "next_cursor": next_cursor.flatten() }).to_string())
        })))
    }
}

/// The job named `id`, or the refusal naming it.
fn find<'b>(book: &'b mut Book, id: &str) -> Result<&'b mut Kept, Refusal> {
    book.jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or_else(|| unknown(id))
}

/// What a call naming no job's id reads back.
fn unknown(id: &str) -> Refusal {
    Refusal::new(code::SCHEDULE_NOT_FOUND, format!("{UNKNOWN} {id}"))
}

/// A refusal where the embedder reads an I/O failure.
fn refused_io(refusal: Refusal) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, refusal.detail)
}
