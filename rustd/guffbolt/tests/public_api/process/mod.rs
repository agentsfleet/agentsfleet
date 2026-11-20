//! The process tools and the environment they run in, through the public
//! API: `shell`, `git`, `exec_command` and `write_stdin` built with
//! `Tools::builder()`, the run's `Sessions`, `LocalEnv` on this host, and the
//! event pipe an environment feeds.
//!
//! A host process shows what `LocalEnv` really does. An environment of the
//! test's own, `Fake`, stands where a sandbox would: it ends a process the
//! way a test says, refuses what a test says, and runs on tokio's paused
//! clock, so a yield or a missing ending is waited out without waiting.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use guffbolt::tools::env::{
    EDGE_BYTES, ENDING_GRACE_DEFAULT, Ending, EntryKind, Env, EnvError, EnvErrorKind, EnvFuture,
    Events as _, Feed, FileContent, Listing, Process, ProcessEvent, ProcessId, Spawn, Stream, pipe,
};
use guffbolt::tools::process::sessions::{SESSIONS_PER_RUN_MAX, Sessions};
use guffbolt::tools::{Context, ContextTool, LocalEnv, ToolsConfig, code};
use guffbolt::{CallRef, ToolOutput, Tools};
use serde_json::{Value, json};

mod env_types;
mod exec_fake;
mod exec_host;
mod git;
mod local_env;
mod pipe;
mod sessions;
mod shell_fake;
mod shell_host;

/// How a process the fake starts goes.
#[derive(Debug)]
enum Act {
    /// It runs until the test ends it.
    Hang,
    /// It says `said` and ends as `ending`, its output left open when
    /// `abandoned`.
    End {
        said: &'static str,
        ending: Ending,
        abandoned: bool,
    },
    /// Its events finish with no ending, as an environment that went away.
    Vanish,
}

/// An environment of the test's own: each process goes as the next act says
/// (or hangs), and every spawn, write and kill is kept.
#[derive(Debug, Default)]
struct Fake {
    acts: Mutex<VecDeque<Act>>,
    refuse_spawn: Option<EnvError>,
    refuse_write: Option<EnvErrorKind>,
    refuse_kill: bool,
    grace: Duration,
    next: AtomicU64,
    feeds: Mutex<BTreeMap<u64, Feed>>,
    spawned: Mutex<Vec<Spawn>>,
    written: Mutex<Vec<(u64, Bytes)>>,
    killed: Mutex<Vec<u64>>,
}

impl Fake {
    fn acting(acts: impl IntoIterator<Item = Act>) -> Self {
        Self {
            acts: Mutex::new(acts.into_iter().collect()),
            ..Self::default()
        }
    }

    /// Process `id` says `text`.
    fn say(&self, id: u64, text: &'static str) {
        let feeds = lock(&self.feeds);
        if let Some(feed) = feeds.get(&id) {
            feed.output(Stream::Stdout, Bytes::from_static(text.as_bytes()));
        }
        drop(feeds);
    }

    /// Process `id` ends as `ending`.
    fn end(&self, id: u64, ending: Ending) {
        let feed = lock(&self.feeds).remove(&id);
        if let Some(feed) = feed {
            feed.end(ending, false);
        }
    }
}

impl Env for Fake {
    fn root(&self) -> &'static str {
        "/work"
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        lock(&self.spawned).push(spawn.clone());
        if let Some(refused) = &self.refuse_spawn {
            return Box::pin(std::future::ready(Err(refused.clone())));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (feed, events) = pipe();
        let act = lock(&self.acts).pop_front().unwrap_or(Act::Hang);
        match act {
            Act::Hang => {
                lock(&self.feeds).insert(id, feed);
            }
            Act::End {
                said,
                ending,
                abandoned,
            } => {
                feed.output(Stream::Stdout, Bytes::from_static(said.as_bytes()));
                feed.end(ending, abandoned);
            }
            Act::Vanish => drop(feed),
        }
        let started = Process {
            id: ProcessId::new(id),
            events: Box::new(events),
        };
        Box::pin(std::future::ready(Ok(started)))
    }

    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()> {
        lock(&self.written).push((process.get(), data));
        let written = self.refuse_write.map_or(Ok(()), |kind| {
            Err(EnvError::new(kind, "the fake refused the write"))
        });
        Box::pin(std::future::ready(written))
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        lock(&self.killed).push(process.get());
        let killed = if self.refuse_kill {
            Err(
                EnvError::new(EnvErrorKind::Unavailable, "the fake refused the kill")
                    .logged_as("fake_kill_refused", "the test asked it to"),
            )
        } else {
            self.end(process.get(), Ending::Signaled(9));
            Ok(())
        };
        Box::pin(std::future::ready(killed))
    }

    fn read_file<'a>(&'a self, _path: &'a str, _max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(std::future::ready(Err(no_files())))
    }

    fn write_file<'a>(&'a self, _path: &'a str, _data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(no_files())))
    }

    fn append_file<'a>(&'a self, _path: &'a str, _data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(no_files())))
    }

    fn delete_file<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(no_files())))
    }

    fn list_dir<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(std::future::ready(Err(no_files())))
    }

    fn ending_grace(&self) -> Duration {
        self.grace
    }
}

fn no_files() -> EnvError {
    EnvError::new(EnvErrorKind::Unavailable, "the fake keeps no files")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The process tools over `env` and `sessions`, `git` in `repository`.
fn tools<'r>(
    env: &'r dyn Env,
    sessions: &'r Sessions,
    repository: Option<&'r str>,
) -> Tools<ContextTool<'r>> {
    let builder = Tools::builder().with_env(env).with_sessions(sessions);
    let builder = match repository {
        Some(repository) => builder.with_repository(repository),
        None => builder,
    };
    let config =
        ToolsConfig::new(["shell", "git", "exec_command", "write_stdin"]).expect("four names");
    builder
        .build(&config)
        .expect("the env and sessions are given")
}

/// One call to `name`, as step 1's first call of run `run-1`.
async fn call(tools: &Tools<ContextTool<'_>>, name: &str, arguments: Value) -> ToolOutput {
    let call_ref = CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    };
    let context = Context {
        call_ref: &call_ref,
        run_id: "run-1",
        wake: None,
    };
    tools.dispatch(name, &arguments, context).await
}

/// A scratch directory and `LocalEnv` rooted at it.
fn local() -> (tempfile::TempDir, LocalEnv) {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let env = LocalEnv::new(directory.path()).expect("the directory exists");
    (directory, env)
}

/// What the model reads of a process that ended as `ending`, after `text`.
const EXITED_0: &str = "Process exited with code 0";
const INTERRUPTED: &str = "Process interrupted before its ending arrived";
const ABANDONED: &str =
    "... the process ended with its output still open; what was written after is not shown ...";
