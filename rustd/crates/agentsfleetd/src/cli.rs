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

use std::time::Duration;

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
    /// Print the `OpenAPI` document this binary's routes generate, and exit.
    ///
    /// Only compiled under `--features openapi`, which the release build never
    /// names — so this is a CI build's verb, not an operator's. It opens
    /// nothing: the document is a function of the route table and the
    /// annotations, so it needs no datastore and no runtime.
    #[cfg(feature = "openapi")]
    Openapi,
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
        #[cfg(feature = "openapi")]
        Some(&Command::Openapi) => openapi(),
        // No subcommand: preflight only. Useful as a container healthcheck and
        // as the thing an operator runs to find out what is missing before
        // taking the trouble to start anything.
        None => check(env),
    }
}

/// How long a finished process waits on blocking work nothing can cancel.
///
/// Dropping a runtime WAITS for every running blocking operation, and a
/// `spawn_blocking` task cannot be cancelled once it has started — dropping
/// its `JoinHandle` detaches it and nothing else. So a caller that bounded its
/// own await has bounded only itself: the work runs on, and the drop at the
/// end of a `block_on` blocks the exit for exactly as long as the caller was
/// trying not to wait.
///
/// The telemetry flush is the case this exists for. It carries its own budget
/// under the supervisor's join deadline, and past that budget the process has
/// already decided the signal is lost — so this is a last resort measured in
/// the time a thread needs to notice, not a second chance for the work.
const BLOCKING_SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// Runs `body` on a runtime from `runtime`, reporting one that would not build.
///
/// The runtime is built here rather than by `#[tokio::main]` so the paths that
/// never need one — a usage error, `--help`, a preflight refusal — do not
/// construct a thread pool on their way to exiting.
///
/// The shutdown is bounded rather than implicit. `Runtime`'s `Drop` waits on
/// the blocking pool with no deadline, so an exporter parked in a synchronous
/// call would hold the process open after every task above it had already
/// given up on it — see [`BLOCKING_SHUTDOWN_GRACE`].
pub fn on_runtime<F>(runtime: RuntimeSource, body: F) -> u8
where
    F: Future<Output = u8>,
{
    match runtime() {
        Ok(runtime) => {
            let status = runtime.block_on(body);
            // Taken by value, so the implicit unbounded drop cannot also run.
            runtime.shutdown_timeout(BLOCKING_SHUTDOWN_GRACE);
            status
        }
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

/// Writes the generated `OpenAPI` document to stdout.
///
/// The one subcommand that is command OUTPUT rather than a log: what a caller
/// asked for IS the document, so it goes to stdout and the exit status says
/// whether it could be written. `public/openapi.json` is this, redirected.
#[cfg(feature = "openapi")]
#[must_use]
pub fn openapi() -> u8 {
    match afd_api::openapi::document().to_pretty_json() {
        Ok(document) => {
            // logging: the document IS this subcommand's output, not a log record
            println!("{document}");
            SUCCESS
        }
        Err(error) => {
            crate::fatal::die(&error);
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
