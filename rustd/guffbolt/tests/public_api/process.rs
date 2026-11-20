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

// ---- shell ---------------------------------------------------------------

/// A clean exit reads as the output alone, run with the commands' variables
/// and none of the harness's own.
#[cfg(unix)]
#[tokio::test]
async fn shell_reads_back_a_clean_exit_as_its_output_alone() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": "printf '%s|%s|%s|%s' \"$LANG\" \"$TERM\" \"$NO_COLOR\" \"${HOME-unset}\""}),
    )
    .await;

    assert_eq!(output.text, "C.UTF-8|dumb|1|unset");
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
}

#[cfg(unix)]
#[tokio::test]
async fn shell_reports_a_failing_exit_after_the_output() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "printf out; exit 3"})).await;

    assert_eq!(output.text, "out\nProcess exited with code 3");
    assert_eq!((output.exit_code, output.error_code), (Some(3), None));
}

#[cfg(unix)]
#[tokio::test]
async fn shell_kills_a_command_at_its_timeout() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": "sleep 30", "timeout_ms": 50}),
    )
    .await;

    assert_eq!(output.text, "Process timed out after 50 ms");
    assert_eq!(
        (output.exit_code, output.error_code),
        (None, Some(code::TIMED_OUT))
    );
}

/// A command a signal ended reads the signal, and the exit status a shell
/// reports for it.
#[cfg(unix)]
#[tokio::test]
async fn shell_reports_a_command_a_signal_ended() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "kill -KILL $$"})).await;

    assert_eq!(output.text, "Process killed by signal 9");
    assert_eq!((output.exit_code, output.error_code), (Some(137), None));
}

#[tokio::test]
async fn shell_refuses_arguments_its_schema_does_not_name() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let unknown = call(&tools, "shell", json!({"command": "true", "cwd": "x"})).await;
    let missing = call(&tools, "shell", json!({})).await;

    assert!(
        unknown
            .text
            .starts_with("[invalid_arguments] unknown field `cwd`"),
        "{}",
        unknown.text
    );
    assert_eq!(missing.text, "[invalid_arguments] missing field `command`");
    assert_eq!(lock(&fake.spawned).len(), 0, "nothing ran");
}

/// The command runs as `sh -c`, with the commands' variables, under the
/// timeout asked for, the default when none is, and never past the ceiling.
#[tokio::test]
async fn shell_runs_sh_c_under_the_timeout_asked_held_to_its_ceiling() {
    let fake = Fake::acting([
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
    ]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let asked = call(
        &tools,
        "shell",
        json!({"command": "true", "timeout_ms": 1500}),
    )
    .await;
    let _default = call(&tools, "shell", json!({"command": "true"})).await;
    let _past = call(
        &tools,
        "shell",
        json!({"command": "true", "timeout_ms": 9_999_999}),
    )
    .await;

    assert_eq!(asked.text, "");
    let spawned = lock(&fake.spawned);
    let limits: Vec<Option<Duration>> = spawned.iter().map(Spawn::time_limit).collect();
    assert_eq!(
        limits,
        [
            Some(Duration::from_millis(1500)),
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(600))
        ]
    );
    let first = spawned.first().expect("a spawn");
    assert_eq!(first.argv(), ["/bin/sh", "-c", "true"]);
    assert_eq!(first.working_directory(), None);
    assert!(!first.on_terminal());
    assert_eq!(
        first
            .environment()
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>(),
        [
            "COLORTERM=",
            "GH_PAGER=cat",
            "GIT_PAGER=cat",
            "LANG=C.UTF-8",
            "LC_ALL=C.UTF-8",
            "LC_CTYPE=C.UTF-8",
            "NO_COLOR=1",
            "PAGER=cat",
            "TERM=dumb",
        ]
    );
}

/// An environment that refuses the spawn answers in its own words, under
/// the code its kind of refusal reads.
#[tokio::test]
async fn shell_on_an_env_that_refuses_the_spawn_reads_the_refusal() {
    let cases = [
        (
            EnvErrorKind::Unavailable,
            "[sandbox_unavailable] the executor is gone",
        ),
        (
            EnvErrorKind::PathRefused,
            "[path_not_allowed] the executor is gone",
        ),
        (
            EnvErrorKind::NotFound,
            "[file_not_found] the executor is gone",
        ),
        (
            EnvErrorKind::UnknownProcess,
            "[sandbox_unavailable] the executor is gone",
        ),
    ];
    let mut read = Vec::new();
    for (kind, _) in &cases {
        let fake = Fake {
            refuse_spawn: Some(EnvError::new(*kind, "the executor is gone")),
            ..Fake::default()
        };
        let sessions = Sessions::default();
        let tools = tools(&fake, &sessions, None);
        let output = call(&tools, "shell", json!({"command": "true"})).await;
        read.push((output.text, output.exit_code));
    }

    let expected: Vec<(String, Option<i32>)> = cases
        .iter()
        .map(|(_, text)| ((*text).to_owned(), None))
        .collect();
    assert_eq!(read, expected);
}

#[tokio::test]
async fn shell_reads_an_out_of_memory_kill_with_its_code() {
    let fake = Fake::acting([Act::End {
        said: "partial",
        ending: Ending::OutOfMemory,
        abandoned: false,
    }]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "big"})).await;

    assert_eq!(output.text, "partial\nProcess killed: out of memory");
    assert_eq!(
        (output.exit_code, output.error_code),
        (Some(137), Some(code::OUT_OF_MEMORY))
    );
}

/// An environment that never says how the command ended is given up on
/// once the timeout and its grace pass; one whose events finish without an
/// ending reads the same.
#[tokio::test(start_paused = true)]
async fn shell_gives_up_on_an_ending_that_never_arrives() {
    let fake = Fake::acting([Act::Hang, Act::Vanish]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let started = tokio::time::Instant::now();
    let hung = call(
        &tools,
        "shell",
        json!({"command": "hang", "timeout_ms": 100}),
    )
    .await;
    let waited = started.elapsed();
    let vanished = call(&tools, "shell", json!({"command": "gone"})).await;

    assert_eq!(hung.text, INTERRUPTED);
    assert_eq!(
        (hung.exit_code, hung.error_code),
        (None, Some(guffbolt::ErrorCode::INTERRUPTED))
    );
    assert_eq!(
        waited,
        Duration::from_millis(100),
        "the timeout, plus the fake's grace of 0"
    );
    assert_eq!(vanished.text, INTERRUPTED);
}

#[tokio::test]
async fn shell_says_last_when_a_commands_output_was_left_open() {
    let fake = Fake::acting([Act::End {
        said: "tail",
        ending: Ending::Exited(0),
        abandoned: true,
    }]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "background"})).await;

    assert_eq!(output.text, format!("tail\n{ABANDONED}"));
    assert_eq!(output.exit_code, Some(0));
}

// ---- git -----------------------------------------------------------------

/// Every subcommand that reaches a remote is refused before anything runs,
/// wherever it sits after git's own options.
#[tokio::test]
async fn git_refuses_each_subcommand_that_reaches_a_remote() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);
    let cases = [
        (json!(["push"]), "push"),
        (json!(["-C", "sub", "fetch"]), "fetch"),
        (json!(["--no-pager", "pull", "origin"]), "pull"),
        (
            json!(["-c", "a.b=c", "--git-dir", "x", "remote", "-v"]),
            "remote",
        ),
        (json!(["clone", "https://example.com/r.git"]), "clone"),
    ];

    let mut read = Vec::new();
    for (args, _) in &cases {
        read.push(call(&tools, "git", json!({"args": args})).await);
    }

    let texts: Vec<&str> = read.iter().map(|output| output.text.as_str()).collect();
    let expected: Vec<String> = cases
        .iter()
        .map(|(_, sub)| {
            format!(
                "[subcommand_not_allowed] git {sub} reaches a remote, which this environment does not reach"
            )
        })
        .collect();
    assert_eq!(texts, expected);
    assert!(
        read.iter()
            .all(|output| output.error_code == Some(code::SUBCOMMAND_NOT_ALLOWED))
    );
    assert_eq!(lock(&fake.spawned).len(), 0, "nothing ran");
}

/// git runs in the repository the embedder named, or at the root; arguments
/// that name no subcommand, or only an option's value, run as written.
#[tokio::test]
async fn git_runs_in_the_named_repository_with_the_commands_variables() {
    let fake = Fake::acting([
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
    ]);
    let sessions = Sessions::default();
    let in_repository = tools(&fake, &sessions, Some("checkout"));
    let at_root = tools(&fake, &sessions, None);

    let _status = call(&in_repository, "git", json!({"args": ["status"]})).await;
    let _version = call(
        &at_root,
        "git",
        json!({"args": ["--version"], "timeout_ms": 20}),
    )
    .await;
    let _dangling = call(&at_root, "git", json!({"args": ["-C"]})).await;

    let spawned = lock(&fake.spawned);
    let seen: Vec<(Vec<String>, Option<String>, Option<Duration>)> = spawned
        .iter()
        .map(|spawn| {
            (
                spawn.argv().to_vec(),
                spawn.working_directory().map(str::to_owned),
                spawn.time_limit(),
            )
        })
        .collect();
    let argv = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        seen,
        [
            (
                argv(&["git", "status"]),
                Some("checkout".to_owned()),
                Some(Duration::from_secs(10))
            ),
            (
                argv(&["git", "--version"]),
                None,
                Some(Duration::from_millis(20))
            ),
            (argv(&["git", "-C"]), None, Some(Duration::from_secs(10))),
        ]
    );
    assert_eq!(
        spawned
            .first()
            .and_then(|spawn| spawn.environment().get("GIT_PAGER").cloned()),
        Some("cat".to_owned())
    );
}

/// `git` on this host, in the repository the embedder named. `LocalEnv`
/// starts git with none of the harness's variables, so no `GIT_DIR` a hook
/// exported reaches it.
#[cfg(unix)]
#[tokio::test]
async fn git_initialises_the_named_repository_on_this_host() {
    let (directory, env) = local();
    std::fs::create_dir(directory.path().join("checkout")).expect("made");
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, Some("checkout"));

    let init = call(&tools, "git", json!({"args": ["init", "-q"]})).await;
    let inside = call(
        &tools,
        "git",
        json!({"args": ["rev-parse", "--is-inside-work-tree"]}),
    )
    .await;
    let missing = call(&tools, "git", json!({"args": ["no-such-subcommand"]})).await;

    assert_eq!((init.text.as_str(), init.exit_code), ("", Some(0)));
    assert!(directory.path().join("checkout/.git").is_dir());
    assert_eq!(inside.text, "true\n");
    assert_eq!(missing.exit_code, Some(1));
    assert!(
        missing.text.ends_with("Process exited with code 1"),
        "{}",
        missing.text
    );
}

// ---- exec_command and write_stdin on this host -----------------------------

/// A command that ends within its yield reads its output and its exit, and
/// leaves no session behind to write to.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_answers_a_quick_command_and_keeps_no_session() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(&tools, "exec_command", json!({"cmd": "printf done"})).await;
    let after = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "x"}),
    )
    .await;

    assert_eq!(output.text, format!("done\n{EXITED_0}"));
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
    assert!(sessions.is_quiescent());
    assert_eq!(
        after.text,
        "[session_not_found] session 1 is not an open session"
    );
    assert_eq!(after.error_code, Some(code::SESSION_NOT_FOUND));
}

/// A process still running when the yield passes stays open as a session;
/// `write_stdin` feeds it and reads what it printed; the run's end closes it.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_keeps_a_running_process_and_write_stdin_feeds_it() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let opened = call(
        &tools,
        "exec_command",
        json!({"cmd": "cat", "yield_time_ms": 250}),
    )
    .await;
    let fed = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "ping\n", "yield_time_ms": 250}),
    )
    .await;
    let still_running = sessions.is_quiescent();
    let closed = sessions.close_all(&env).await;

    assert_eq!(opened.text, "Process running with session ID 1");
    assert_eq!((opened.exit_code, opened.error_code), (None, None));
    assert_eq!(fed.text, "ping\nProcess running with session ID 1");
    assert!(!still_running);
    assert_eq!(closed, 1, "one session was still running");
    assert!(sessions.is_quiescent());
}

/// A process that closed its input takes no more: the write reads why, and
/// the process runs on.
#[cfg(unix)]
#[tokio::test]
async fn write_stdin_to_a_process_that_closed_its_input_reads_the_refusal() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let opened = call(
        &tools,
        "exec_command",
        json!({"cmd": "exec 0<&-; sleep 30", "yield_time_ms": 250}),
    )
    .await;
    // The first write is queued, and fails on the closed pipe; the second
    // finds the queue gone.
    let _first = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "a", "yield_time_ms": 250}),
    )
    .await;
    let second = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "b", "yield_time_ms": 250}),
    )
    .await;
    let closed = sessions.close_all(&env).await;

    assert_eq!(opened.text, "Process running with session ID 1");
    assert_eq!(
        second.text,
        "the write was not taken: the process closed its input, or has not read what it was \
         sent\nProcess running with session ID 1"
    );
    assert_eq!(second.error_code, None);
    assert_eq!(closed, 1);
}

/// On a pseudo-terminal when the model asks for one, on pipes otherwise.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_runs_on_a_terminal_only_when_asked() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);
    let probe = "if test -t 0; then printf terminal; else printf pipes; fi";

    let on_terminal = call(&tools, "exec_command", json!({"cmd": probe, "tty": true})).await;
    let on_pipes = call(&tools, "exec_command", json!({"cmd": probe})).await;

    assert!(
        on_terminal.text.starts_with("terminal"),
        "{}",
        on_terminal.text
    );
    assert_eq!(on_terminal.exit_code, Some(0));
    assert_eq!(on_pipes.text, format!("pipes\n{EXITED_0}"));
}

/// `workdir` is read under the root: a directory there runs the command, one
/// that leaves the root is refused, and one that is not there fails the
/// spawn. A refused start frees the place it held.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_runs_in_its_workdir_and_refuses_one_outside_the_root() {
    let (directory, env) = local();
    std::fs::create_dir(directory.path().join("sub")).expect("made");
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let inside = call(
        &tools,
        "exec_command",
        json!({"cmd": "pwd", "workdir": "sub"}),
    )
    .await;
    let outside = call(
        &tools,
        "exec_command",
        json!({"cmd": "pwd", "workdir": "../"}),
    )
    .await;
    let absent = call(
        &tools,
        "exec_command",
        json!({"cmd": "pwd", "workdir": "missing"}),
    )
    .await;

    let root = env.root().to_owned();
    assert_eq!(inside.text, format!("{root}/sub\n{EXITED_0}"));
    assert_eq!(
        outside.text,
        "[path_not_allowed] the path leaves the working directory"
    );
    assert!(
        absent.text.starts_with("[file_not_found] "),
        "{}",
        absent.text
    );
    assert!(sessions.is_quiescent(), "no refused start holds a place");
}

// ---- exec_command and write_stdin on the fake ------------------------------

/// A process that ended between calls answers the next write with its
/// ending, and the write is never sent.
#[tokio::test(start_paused = true)]
async fn write_stdin_reads_the_ending_of_a_process_that_ended_between_calls() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let opened = call(
        &tools,
        "exec_command",
        json!({"cmd": "job", "yield_time_ms": 250}),
    )
    .await;
    fake.say(1, "bye");
    fake.end(1, Ending::Exited(4));
    let late = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "late"}),
    )
    .await;

    assert_eq!(opened.text, "Process running with session ID 1");
    assert_eq!(late.text, "bye\nProcess exited with code 4");
    assert_eq!(late.exit_code, Some(4));
    assert_eq!(lock(&fake.written).len(), 0, "the write was never sent");
    assert!(sessions.is_quiescent());
}

/// Each way an environment refuses a write reads as the session's own
/// answer, and a session the write could not reach stays open.
#[tokio::test(start_paused = true)]
async fn write_stdin_reads_each_way_a_write_is_refused() {
    let cases = [
        (
            EnvErrorKind::UnknownProcess,
            "the write was not delivered: the process had already ended, and its ending \
             follows\nProcess running with session ID 1",
        ),
        (
            EnvErrorKind::InputRefused,
            "the write was not taken: the process closed its input, or has not read what it \
             was sent\nProcess running with session ID 1",
        ),
        (
            EnvErrorKind::Unavailable,
            "[sandbox_unavailable] the fake refused the write",
        ),
    ];
    let mut read = Vec::new();
    for (kind, _) in &cases {
        let fake = Fake {
            refuse_write: Some(*kind),
            ..Fake::default()
        };
        let sessions = Sessions::default();
        let tools = tools(&fake, &sessions, None);
        let _opened = call(&tools, "exec_command", json!({"cmd": "job"})).await;
        let output = call(
            &tools,
            "write_stdin",
            json!({"session_id": 1, "chars": "x"}),
        )
        .await;
        read.push((
            output.text,
            sessions.is_quiescent(),
            lock(&fake.written).len(),
        ));
    }

    let expected: Vec<(String, bool, usize)> = cases
        .iter()
        .map(|(_, text)| ((*text).to_owned(), false, 1))
        .collect();
    assert_eq!(read, expected);
}

/// A call waits its yield, held between 250 ms and 30 s; an empty write,
/// which only reads, waits at least 5 s.
#[tokio::test(start_paused = true)]
async fn a_calls_yield_is_held_between_its_floor_and_thirty_seconds() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);
    let timed = |name: &'static str, arguments: Value| {
        let tools = &tools;
        async move {
            let started = tokio::time::Instant::now();
            let _output = call(tools, name, arguments).await;
            started.elapsed()
        }
    };

    let short = timed("exec_command", json!({"cmd": "a", "yield_time_ms": 1})).await;
    let default = timed("exec_command", json!({"cmd": "b"})).await;
    let long = timed("exec_command", json!({"cmd": "c", "yield_time_ms": 99_999})).await;
    let empty = timed("write_stdin", json!({"session_id": 1, "yield_time_ms": 1})).await;
    let written = timed(
        "write_stdin",
        json!({"session_id": 1, "chars": "x", "yield_time_ms": 1}),
    )
    .await;

    assert_eq!(
        [short, default, long, empty, written],
        [
            Duration::from_millis(250),
            Duration::from_secs(10),
            Duration::from_secs(30),
            Duration::from_secs(5),
            Duration::from_millis(250),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn exec_command_reads_an_out_of_memory_kill_and_closes_the_session() {
    let fake = Fake::acting([Act::End {
        said: "half",
        ending: Ending::OutOfMemory,
        abandoned: false,
    }]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "exec_command", json!({"cmd": "big", "tty": true})).await;

    assert_eq!(output.text, "half\nProcess killed: out of memory");
    assert_eq!(
        (output.exit_code, output.error_code),
        (Some(137), Some(code::OUT_OF_MEMORY))
    );
    assert!(sessions.is_quiescent());
    assert!(lock(&fake.spawned).first().is_some_and(Spawn::on_terminal));
}

#[tokio::test]
async fn exec_command_on_an_env_that_refuses_the_spawn_holds_no_place() {
    let fake = Fake {
        refuse_spawn: Some(EnvError::new(EnvErrorKind::Unavailable, "no room")),
        ..Fake::default()
    };
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "exec_command", json!({"cmd": "x"})).await;

    assert_eq!(output.text, "[sandbox_unavailable] no room");
    assert!(sessions.is_quiescent());
}

// ---- Sessions ------------------------------------------------------------

/// Opens `count` hanging sessions on `fake`, ids 1 to `count`.
async fn open_hanging(sessions: &Sessions, fake: &Fake, count: usize) {
    for _ in 0..count {
        let slot = sessions.make_room(fake).await;
        let process = fake
            .spawn(&Spawn::program("hang"))
            .await
            .expect("the fake starts it");
        let _shared = sessions.open(slot, process);
    }
}

/// At the cap, room is made from the least recently used session that
/// already ended, without a kill.
#[tokio::test]
async fn at_the_cap_room_is_made_from_an_ended_session_first() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    open_hanging(&sessions, &fake, SESSIONS_PER_RUN_MAX).await;
    fake.end(5, Ending::Exited(0));

    let _slot = sessions.make_room(&fake).await;

    assert!(
        sessions.get(ProcessId::new(5)).is_none(),
        "the ended one went"
    );
    assert!(
        sessions.get(ProcessId::new(1)).is_some(),
        "the oldest running one stayed"
    );
    assert_eq!(lock(&fake.killed).len(), 0);
}

/// With none ended, the least recently used running session is killed; one
/// a call is reading, and one used since, are passed over.
#[tokio::test]
async fn at_the_cap_the_least_recently_used_running_session_is_killed() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    open_hanging(&sessions, &fake, SESSIONS_PER_RUN_MAX).await;
    let read = sessions.get(ProcessId::new(1)).expect("open");
    let reading = read.lock().await;
    let _used = sessions.get(ProcessId::new(2)).expect("open");

    let _slot = sessions.make_room(&fake).await;

    assert_eq!(*lock(&fake.killed), [3]);
    assert!(sessions.get(ProcessId::new(3)).is_none());
    assert!(
        sessions.get(ProcessId::new(1)).is_some(),
        "the one being read stayed"
    );
    drop(reading);
}

/// The run's end forgets an ended session, kills a running one, and counts
/// a kill the environment refused as killed all the same.
#[tokio::test]
async fn close_all_kills_the_running_sessions_and_forgets_the_ended() {
    let fake = Fake {
        refuse_kill: true,
        ..Fake::default()
    };
    let sessions = Sessions::default();
    open_hanging(&sessions, &fake, 3).await;
    fake.end(2, Ending::Exited(0));

    let running = sessions.close_all(&fake).await;

    assert_eq!(running, 2);
    assert_eq!(*lock(&fake.killed), [1, 3]);
    assert!(sessions.is_quiescent());
    assert_eq!(sessions.close_all(&fake).await, 0, "nothing is left");
}

/// A place held and dropped unopened, as a cancelled call leaves it, is
/// freed; a session closed twice is forgotten once.
#[tokio::test]
async fn a_dropped_place_is_freed_and_a_session_closes_once() {
    let fake = Fake::default();
    let sessions = Sessions::default();

    let slot = sessions.make_room(&fake).await;
    let held = sessions.is_quiescent();
    drop(slot);
    let freed = sessions.is_quiescent();
    open_hanging(&sessions, &fake, 1).await;
    sessions.close(ProcessId::new(1), Ending::Exited(0));
    sessions.close(ProcessId::new(1), Ending::Exited(0));

    assert!(!held);
    assert!(freed);
    assert!(sessions.get(ProcessId::new(1)).is_none());
    assert!(sessions.is_quiescent());
}

// ---- the environment's own types -------------------------------------------

#[test]
fn an_ending_names_its_kind_and_the_exit_status_a_shell_reports() {
    let endings = [
        Ending::Exited(3),
        Ending::Signaled(15),
        Ending::Signaled(i32::MAX),
        Ending::TimedOut,
        Ending::OutOfMemory,
        Ending::Interrupted,
    ];

    let read: Vec<(&str, Option<i32>)> = endings
        .iter()
        .map(|ending| (ending.kind(), ending.exit_code()))
        .collect();

    assert_eq!(
        read,
        [
            ("exited", Some(3)),
            ("signaled", Some(143)),
            ("signaled", Some(i32::MAX)),
            ("timed_out", None),
            ("out_of_memory", Some(137)),
            ("interrupted", None),
        ]
    );
}

#[test]
fn a_spawn_keeps_what_it_was_built_with() {
    let spawn = Spawn::program("tool")
        .arg("one")
        .args(["two", "three"])
        .cwd("sub")
        .env("KEY", "value")
        .terminal()
        .timeout(Duration::from_secs(2));

    assert_eq!(spawn.argv(), ["tool", "one", "two", "three"]);
    assert_eq!(spawn.working_directory(), Some("sub"));
    assert_eq!(
        spawn.environment().get("KEY").map(String::as_str),
        Some("value")
    );
    assert!(spawn.on_terminal());
    assert_eq!(spawn.time_limit(), Some(Duration::from_secs(2)));
    assert_eq!(ProcessId::new(7).get(), 7);
}

#[test]
fn an_env_error_reads_its_kind_and_what_its_log_line_records() {
    let plain = EnvError::new(EnvErrorKind::NotFound, "gone");
    let logged = EnvError::new(EnvErrorKind::InputRefused, "full")
        .logged_as("executor_backpressure", "the socket's buffer is full");
    let kinds = [
        EnvErrorKind::PathRefused,
        EnvErrorKind::NotFound,
        EnvErrorKind::UnknownProcess,
        EnvErrorKind::InputRefused,
        EnvErrorKind::Unavailable,
    ];

    let predicates: Vec<[bool; 4]> = kinds
        .iter()
        .map(|kind| {
            let error = EnvError::new(*kind, "x");
            [
                error.is_path_refused(),
                error.is_not_found(),
                error.is_unknown_process(),
                error.is_input_refused(),
            ]
        })
        .collect();

    assert_eq!(
        (
            plain.kind(),
            plain.message(),
            plain.log_code(),
            plain.log_reason()
        ),
        (EnvErrorKind::NotFound, "gone", "env_failed", "gone")
    );
    assert_eq!(plain.to_string(), "gone");
    assert_eq!(
        (logged.message(), logged.log_code(), logged.log_reason()),
        (
            "full",
            "executor_backpressure",
            "the socket's buffer is full"
        )
    );
    assert_eq!(
        predicates,
        [
            [true, false, false, false],
            [false, true, false, false],
            [false, false, true, false],
            [false, false, false, true],
            [false, false, false, false],
        ]
    );
}

// ---- the event pipe --------------------------------------------------------

/// Every event a reader takes from `events` as it stands.
fn drained(events: &mut guffbolt::tools::env::Piped) -> (Vec<ProcessEvent>, bool) {
    events.take_ready()
}

fn output(data: Vec<u8>) -> ProcessEvent {
    ProcessEvent::Output {
        stream: Stream::Stdout,
        data: Bytes::from(data),
    }
}

/// A chunk that crosses the head's edge is cut there, on a character
/// boundary, or where the cap falls for bytes that are not text; nothing is
/// lost while the tail holds it.
#[test]
fn a_chunk_across_the_heads_edge_is_cut_there_and_read_whole() {
    let (feed, mut events) = pipe();
    let mut chunk = vec![b'a'; EDGE_BYTES - 1];
    chunk.push(0xFF);
    chunk.extend_from_slice(b"zzzzz");

    feed.output(Stream::Stdout, Bytes::from(chunk));
    feed.output(Stream::Stdout, Bytes::new());
    feed.end(Ending::Exited(0), false);
    let (read, finished) = drained(&mut events);

    let mut head = vec![b'a'; EDGE_BYTES - 1];
    head.push(0xFF);
    assert_eq!(
        read,
        [
            output(head),
            output(b"zzzzz".to_vec()),
            ProcessEvent::Ended {
                ending: Ending::Exited(0),
                output_abandoned: false
            },
        ],
        "the empty chunk was never kept"
    );
    assert!(finished);
}

/// A character the head's last chunk started, whose rest fell into the gap,
/// joins the gap rather than reading as noise before it.
#[test]
fn a_character_cut_by_the_gap_joins_the_gap() {
    let (feed, mut events) = pipe();
    feed.output(Stream::Stdout, Bytes::from(vec![b'a'; EDGE_BYTES - 2]));
    feed.output(Stream::Stdout, Bytes::from_static(&[0xC3]));
    let mut overflow = vec![0xC3, 0xA9];
    overflow.extend(vec![b'b'; EDGE_BYTES]);
    feed.output(Stream::Stdout, Bytes::from(overflow));
    drop(feed);

    let (read, finished) = drained(&mut events);

    assert_eq!(
        read,
        [
            output(vec![b'a'; EDGE_BYTES - 2]),
            ProcessEvent::Omitted { bytes: 3 },
            output(vec![b'b'; EDGE_BYTES]),
        ]
    );
    assert!(
        finished,
        "a dropped feed finishes the events with no ending"
    );
}

/// `recv` waits for what the feed says next; `Process::ended` reads output
/// to the ending, passing over a gap, and none when the events finish first.
#[tokio::test]
async fn a_reader_waits_for_the_feed_and_reads_to_the_ending() {
    let (feed, mut events) = pipe();
    let reader = tokio::spawn(async move {
        let first = events.recv().await;
        events.disconnect();
        (first, events)
    });
    tokio::task::yield_now().await;
    feed.output(Stream::Stderr, Bytes::from_static(b"late"));
    let (first, events) = reader.await.expect("the reader ran");
    feed.output(Stream::Stdout, Bytes::from(vec![b'x'; EDGE_BYTES + 10]));
    feed.output(Stream::Stdout, Bytes::from(vec![b'y'; EDGE_BYTES]));
    feed.end(Ending::Signaled(15), true);
    let mut heard = Vec::new();

    let ending = Process {
        id: ProcessId::new(1),
        events: Box::new(events),
    }
    .ended(|stream, data| heard.push((stream, data.len())))
    .await;
    let (gone, nothing) = pipe();
    drop(gone);
    let none = Process {
        id: ProcessId::new(2),
        events: Box::new(nothing),
    }
    .ended(|_, _| {})
    .await;

    assert_eq!(
        first,
        Some(ProcessEvent::Output {
            stream: Stream::Stderr,
            data: Bytes::from_static(b"late")
        })
    );
    assert_eq!(ending, Some(Ending::Signaled(15)));
    assert_eq!(
        heard,
        [
            (Stream::Stdout, EDGE_BYTES - 4),
            (Stream::Stdout, EDGE_BYTES)
        ],
        "the head (short of what was read before), then the tail; the gap is passed over"
    );
    assert_eq!(none, None);
}

// ---- LocalEnv ------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn local_env_names_its_root_and_takes_the_defaults_it_does_not_override() {
    let (directory, env) = local();
    let canonical = std::fs::canonicalize(directory.path()).expect("resolves");

    assert_eq!(env.root(), canonical.display().to_string());
    assert_eq!(env.ending_grace(), ENDING_GRACE_DEFAULT);
    assert_eq!(
        env.browser().err(),
        Some("has no browser in this environment")
    );
}

/// Files under the root are written with the directories they need, read to
/// a limit, added to, listed and deleted; a path given absolute under the
/// root reads the same as a relative one.
#[cfg(unix)]
#[tokio::test]
async fn local_env_writes_reads_appends_lists_and_deletes_files_under_its_root() {
    let (directory, env) = local();

    env.write_file("notes/today.txt", Bytes::from_static(b"first"))
        .await
        .expect("written");
    env.append_file("notes/today.txt", Bytes::from_static(b", second"))
        .await
        .expect("added");
    env.append_file("logs/new.txt", Bytes::from_static(b"made"))
        .await
        .expect("made by appending");
    let whole = env.read_file("notes/today.txt", 1024).await.expect("read");
    let absolute = format!("{}/notes/today.txt", env.root());
    let cut = env
        .read_file(&absolute, 5)
        .await
        .expect("read through its absolute path");
    std::fs::create_dir(directory.path().join("notes/inner")).expect("made");
    std::os::unix::fs::symlink("today.txt", directory.path().join("notes/link")).expect("linked");
    let _socket = std::os::unix::net::UnixListener::bind(directory.path().join("notes/sock"))
        .expect("a socket");
    let listing = env.list_dir("notes").await.expect("listed");
    env.delete_file("notes/today.txt").await.expect("deleted");

    assert_eq!(
        (whole.data.as_ref(), whole.truncated),
        (b"first, second".as_slice(), false)
    );
    assert_eq!(
        (cut.data.as_ref(), cut.truncated),
        (b"first".as_slice(), true)
    );
    let mut kinds: Vec<(String, EntryKind)> = listing
        .entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.kind))
        .collect();
    kinds.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        kinds,
        [
            ("inner".to_owned(), EntryKind::Directory),
            ("link".to_owned(), EntryKind::Symlink),
            ("sock".to_owned(), EntryKind::Other),
            ("today.txt".to_owned(), EntryKind::File),
        ]
    );
    let file = listing
        .entries
        .iter()
        .find(|entry| entry.name == "today.txt");
    assert_eq!(file.map(|entry| entry.size), Some(13));
    assert!(!listing.truncated);
    assert!(!directory.path().join("notes/today.txt").exists());
    assert_eq!(
        std::fs::read(directory.path().join("logs/new.txt")).expect("kept"),
        b"made"
    );
}

/// A path that leaves the root, by `..`, by an absolute path elsewhere or
/// through a link, is refused before anything is touched.
#[cfg(unix)]
#[tokio::test]
async fn local_env_refuses_every_path_that_leaves_its_root() {
    let (directory, env) = local();
    let outside = tempfile::tempdir().expect("a second directory");
    std::fs::write(outside.path().join("secret"), b"x").expect("written");
    std::os::unix::fs::symlink(outside.path(), directory.path().join("away")).expect("linked");

    let refusals = [
        env.read_file("../secret", 10).await.err(),
        env.read_file("/etc/hosts", 10).await.err(),
        env.read_file("away/secret", 10).await.err(),
        env.write_file("away/new", Bytes::from_static(b"x"))
            .await
            .err(),
        env.append_file("away/deeper/new", Bytes::from_static(b"x"))
            .await
            .err(),
        env.delete_file("away/secret").await.err(),
        env.list_dir("away").await.err(),
    ];

    let kinds: Vec<Option<(EnvErrorKind, String)>> = refusals
        .iter()
        .map(|refused| {
            refused
                .as_ref()
                .map(|error| (error.kind(), error.message().to_owned()))
        })
        .collect();
    let refused = Some((
        EnvErrorKind::PathRefused,
        "the path leaves the working directory".to_owned(),
    ));
    assert_eq!(kinds, vec![refused; 7]);
    assert!(!outside.path().join("new").exists());
    assert!(outside.path().join("secret").exists());
}

/// Each file call that fails reads the system's own words, sorted: a name
/// that is not there apart from everything else.
#[cfg(unix)]
#[tokio::test]
async fn local_env_sorts_each_failed_file_call() {
    use EnvErrorKind::{NotFound, Unavailable};

    let (directory, env) = local();
    std::fs::write(directory.path().join("file"), b"x").expect("written");
    std::fs::create_dir(directory.path().join("dir")).expect("made");
    std::os::unix::fs::symlink("file", directory.path().join("link")).expect("linked");

    let kinds = [
        env.read_file("missing", 10)
            .await
            .err()
            .map(|error| error.kind()),
        env.read_file("dir", 10)
            .await
            .err()
            .map(|error| error.kind()),
        env.write_file("file/child", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.write_file("dir", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.append_file("file/child", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.append_file("dir", Bytes::from_static(b"x"))
            .await
            .err()
            .map(|error| error.kind()),
        env.delete_file("missing")
            .await
            .err()
            .map(|error| error.kind()),
        env.list_dir("missing")
            .await
            .err()
            .map(|error| error.kind()),
        env.list_dir("file").await.err().map(|error| error.kind()),
    ];
    let not_regular = [
        env.delete_file("dir")
            .await
            .err()
            .map(|error| error.message().to_owned()),
        env.delete_file("link")
            .await
            .err()
            .map(|error| error.message().to_owned()),
    ];

    assert_eq!(
        kinds,
        [
            Some(NotFound),
            Some(Unavailable),
            Some(Unavailable),
            Some(Unavailable),
            Some(Unavailable),
            Some(Unavailable),
            Some(NotFound),
            Some(NotFound),
            Some(Unavailable),
        ]
    );
    assert_eq!(
        not_regular,
        [
            Some("dir is not a regular file".to_owned()),
            Some("link is not a regular file".to_owned())
        ]
    );
    assert!(
        directory.path().join("file").exists(),
        "the link's target stayed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn local_env_lists_at_most_a_thousand_entries() {
    let (directory, env) = local();
    for number in 0..1001 {
        std::fs::write(directory.path().join(format!("f{number}")), b"").expect("written");
    }

    let listing = env.list_dir(".").await.expect("listed");

    assert_eq!(listing.entries.len(), 1000);
    assert!(listing.truncated);
}

/// A write or a kill naming a process the environment does not hold is
/// refused; a kill of a running one ends it by signal, and a process the
/// environment started takes what is written to it.
#[cfg(unix)]
#[tokio::test]
async fn local_env_writes_to_and_kills_only_the_processes_it_holds() {
    let (_directory, env) = local();
    let unknown = ProcessId::new(99);

    let write = env.write(unknown, Bytes::from_static(b"x")).await.err();
    let kill = env.kill(unknown).await.err();
    let echo = env
        .spawn(&Spawn::program("head").arg("-c").arg("2"))
        .await
        .expect("started");
    env.write(echo.id, Bytes::from_static(b"hi"))
        .await
        .expect("queued");
    let mut said = Vec::new();
    let echoed = echo
        .ended(|_stream, data| said.extend_from_slice(&data))
        .await;
    let sleeper = env
        .spawn(&Spawn::program("sleep").arg("30"))
        .await
        .expect("started");
    env.kill(sleeper.id).await.expect("held");
    let killed = sleeper.ended(|_, _| {}).await;

    let unknown_process = Some((EnvErrorKind::UnknownProcess, "no such process".to_owned()));
    let read =
        |error: Option<EnvError>| error.map(|error| (error.kind(), error.message().to_owned()));
    assert_eq!(read(write), unknown_process);
    assert_eq!(read(kill), unknown_process);
    assert_eq!(
        (echoed, said.as_slice()),
        (Some(Ending::Exited(0)), b"hi".as_slice())
    );
    assert_eq!(killed, Some(Ending::Signaled(9)));
}
