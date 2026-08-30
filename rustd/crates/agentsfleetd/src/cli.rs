//! The command line, derived — and what each subcommand does once parsed.
//!
//! # Why this is not in `main.rs`
//!
//! It used to be. `main` read `std::env::args().nth(1)`, matched two string
//! constants, and took the port from the `PORT` variable — a transliteration of
//! `cmd/serve_args.zig`, which hand-parses argv because Zig ships no argument
//! parser. Rust does, so the port that could not be given on the command line,
//! the missing `--help`, and the missing `--version` were all self-inflicted.
//!
//! [`Cli`] replaces that loop, and living in the library rather than beside
//! `main` is what makes every arm of it reachable from a suite: a binary
//! crate's internals cannot be linked by an integration test, and the dispatch
//! is exactly the part worth testing.
//!
//! # The two things `main` still owns
//!
//! Reading the real environment and setting a process status. Everything else —
//! parsing, dispatch, building the runtime, turning an outcome into a status —
//! takes its inputs as parameters, which is why [`RuntimeSource`] is a `fn`
//! pointer rather than a call.

use afd_core::env::EnvSource;
use clap::{Parser, Subcommand};

use crate::daemon::Outcome;
use crate::serve::{DEFAULT_PORT, PORT_KNOB};

/// The status of a process that did what it was asked.
pub const SUCCESS: u8 = 0;

/// The status of a refusal, or of a run that ended badly.
///
/// One, not two: an init system restarts on this and must not restart on a
/// usage error, which `clap` exits 2 for.
pub const FAILURE: u8 = 1;

/// `agentsfleetd` — the agentsfleet control-plane daemon.
///
/// `about` is spelled out rather than taken from the package `description`:
/// that string describes the crate to a Rust developer reading a manifest, and
/// this one is read by an operator deciding what to type next.
#[derive(Debug, Parser)]
#[command(
    name = "agentsfleetd",
    version,
    about = "The agentsfleet control-plane daemon.",
    long_about = None,
    before_help = crate::nameplate::for_help(),
)]
pub struct Cli {
    /// What to do. Omitted, the environment is checked and reported.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Suppress the startup nameplate entirely.
    ///
    /// Separate from `--quiet` on purpose: this one is about the nameplate and
    /// nothing else, so an operator who wants the plain line but not the
    /// decoration is not forced to change anything else about the run.
    #[arg(long, global = true)]
    pub no_banner: bool,

    /// Reduce startup decoration to a single plain line.
    ///
    /// One of the five conditions [`crate::nameplate::Style`] falls back on,
    /// and the only one that is a deliberate request rather than a property of
    /// the destination.
    #[arg(long, global = true)]
    pub quiet: bool,
}

/// What `agentsfleetd` was asked to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Boot every dependency, serve until signalled, then stop the fleet.
    Serve {
        /// The TCP port to bind.
        ///
        /// `0` is rejected rather than accepted as "let the kernel choose": a
        /// daemon nobody can find the port of is not serving, and
        /// `serve_args.zig` rejected it for the same reason.
        #[arg(
            long,
            value_name = "PORT",
            env = PORT_KNOB,
            default_value_t = DEFAULT_PORT,
            value_parser = clap::value_parser!(u16).range(1..),
        )]
        port: u16,
    },
    /// Apply every migration this binary carries, report what moved, exit.
    Migrate,
}

/// How the runtime is built, as a parameter rather than a call.
///
/// The only interesting thing about runtime construction is what happens when
/// it fails, and a real runtime does not fail on demand — so the constructor is
/// injected, exactly as [`crate::serve::serve_accepts`] injects an `Acceptor`.
pub type RuntimeSource = fn() -> std::io::Result<tokio::runtime::Runtime>;

/// Runs `cli` against `env`, reporting the status the process should exit with.
///
/// `signal` is taken rather than built so a suite can hand it one that has
/// already resolved. In the binary it is [`crate::signal::shutdown`], and
/// building it costs nothing on the paths that never poll it.
pub fn run<E, F>(cli: &Cli, env: &E, runtime: RuntimeSource, signal: F) -> u8
where
    E: EnvSource + ?Sized,
    F: Future<Output = ()>,
{
    match cli.command.as_ref() {
        Some(&Command::Serve { port }) => on_runtime(runtime, serve(env, port, signal)),
        Some(&Command::Migrate) => on_runtime(runtime, migrate(env)),
        // No subcommand: preflight only. Useful as a container healthcheck and
        // as the thing an operator runs to find out what is missing before
        // taking the trouble to start anything.
        None => check(env),
    }
}

/// Runs `body` on a runtime from `runtime`, reporting one that would not build.
///
/// The runtime is built here rather than by `#[tokio::main]` so the paths that
/// never need one — a usage error, `--help`, a preflight refusal — do not
/// construct a thread pool on their way to exiting.
pub fn on_runtime<F>(runtime: RuntimeSource, body: F) -> u8
where
    F: Future<Output = u8>,
{
    match runtime() {
        Ok(runtime) => runtime.block_on(body),
        Err(error) => {
            crate::fatal::die(&error);
            FAILURE
        }
    }
}

/// Boots, serves until `signal`, and reports how the fleet stopped.
pub async fn serve<E, F>(env: &E, port: u16, signal: F) -> u8
where
    E: EnvSource + ?Sized,
    F: Future<Output = ()>,
{
    match crate::serve::run(env, port, signal).await {
        Ok(outcome) => status_for(&outcome),
        Err(failure) => {
            crate::fatal::die(&failure);
            FAILURE
        }
    }
}

/// The status a completed serve reports.
///
/// An unclean stop is a failure so an operator sees it in the exit status and
/// not only in a log line they have to go and find.
#[must_use]
pub fn status_for(outcome: &Outcome) -> u8 {
    if outcome.is_clean() {
        SUCCESS
    } else {
        // A task that would not stop, or a server that fell over. This is a
        // LOG, not command output: it describes how the daemon ended rather
        // than answering the operator's request, and the exit status above is
        // what the caller reads. Hoisted per §8A — the `log` bridge duplicates
        // field expressions and llvm-cov scores the dead copy.
        let detail = format!("{outcome:?}");
        tracing::error!(outcome = detail, event = "serve_stopped_unclean");
        FAILURE
    }
}

/// Applies the schema and reports which versions moved.
///
/// The summary is a LOG rather than command output, and that is a correction
/// rather than a preference: every step that produced it already emits one —
/// `migrate_conn_acquired`, `migrate_lock_acquired`, `migrate_refused_schema_ahead`
/// — so a summary on stdout was the one part of this path that could not be
/// queried beside the events it summarises. The contract this command answers
/// on is its EXIT STATUS, which is unchanged and is what the lane asserts.
pub async fn migrate<E: EnvSource + ?Sized>(env: &E) -> u8 {
    match crate::migrate::migrate(env).await {
        Ok(applied) => {
            // Hoisted per §8A — the `log` bridge duplicates field expressions
            // and llvm-cov scores the dead copy.
            let summary = crate::migrate::summarise(&applied);
            tracing::info!(summary, event = "migrate_completed");
            SUCCESS
        }
        Err(failure) => {
            crate::fatal::die(&failure);
            FAILURE
        }
    }
}

/// Resolves the environment and reports it, without opening anything.
pub fn check<E: EnvSource + ?Sized>(env: &E) -> u8 {
    match crate::preflight::preflight(env) {
        Ok(config) => {
            let roles = [
                format!("postgres:{}", config.api_pool().role().tag()),
                format!("redis:{}", config.redis().role().tag()),
            ];
            crate::banner::show(env!("CARGO_PKG_VERSION"), &roles, std::process::id());
            SUCCESS
        }
        Err(refusal) => {
            crate::fatal::die(&refusal);
            FAILURE
        }
    }
}
