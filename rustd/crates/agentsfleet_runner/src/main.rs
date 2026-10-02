//! `agentsfleet-runner`: the one runner binary.
//!
//! Three entries, each only composing library crates:
//!
//! - `run` supervises this host's leases, as the systemd unit.
//! - `probe` prints what this host's kernel can enforce, as a heartbeat would
//!   carry it.
//! - `sandbox` is started by the engine inside each sandbox: it hardens itself
//!   before any thread exists, then serves the executor. The binary is bound
//!   read-only into every sandbox, so there is no second artifact to ship.

use std::process::ExitCode;

use afd_core::env::ProcessEnv;
use afd_core::error_code;
use afr_supervisor::{Config, StorageHome};
use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;

/// Where a record goes when nobody chose.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Why `run` will not start in this build.
const NO_AGENT_ENGINE: &str = "this build carries no agent engine, so it takes no leases; the agent loop and its model providers arrive with their own runner workstream";

/// What `run` logs when it will not start.
const EVENT_RUN_REFUSED: &str = "run_refused";

/// Exit status for an entry this build refuses: distinct from a failure, so a
/// service manager does not restart it in a loop.
const REFUSED: u8 = 2;

/// The runner's command line.
#[derive(Debug, Parser)]
#[command(name = "agentsfleet-runner", version, about)]
struct Cli {
    /// Which entry to run.
    #[command(subcommand)]
    command: Command,
}

/// The binary's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
enum Command {
    /// Supervise this host's leases.
    Run,
    /// Print what this host's kernel can enforce.
    Probe,
    /// Harden this process and serve the executor; started inside each sandbox.
    Sandbox,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        // First and alone: hardening refuses once a second thread exists, so
        // nothing may start one before it.
        Command::Sandbox => sandbox(),
        Command::Probe => probe(),
        Command::Run => run(),
    }
}

/// Hardens and serves; any refusal goes to stderr, which the engine reads.
fn sandbox() -> ExitCode {
    match afr_sandbox::serve_sandboxed() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => refused(&error),
    }
}

/// Writes a refusal to stderr and fails.
///
/// Stderr rather than a subscriber: inside a sandbox no subscriber may start a
/// thread before hardening, and the engine quotes this stream when it refuses.
fn refused(error: &impl std::fmt::Display) -> ExitCode {
    // logging: stderr is the refusal the engine and the operator read
    eprintln!("{error}");
    ExitCode::FAILURE
}

/// Prints the host's capability report and self-test checks as JSON.
///
/// Succeeds only when this host can build a sandbox.
fn probe() -> ExitCode {
    let host = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
    let answer = afr_supervisor::capability::probe_answer(&host);
    match serde_json::to_string_pretty(&answer) {
        Ok(rendered) => {
            // logging: stdout is this command's answer
            println!("{rendered}");
        }
        Err(error) => return refused(&error),
    }
    if host.missing().is_none() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Boots as far as an agent engine is needed, then refuses.
///
/// Every check a real start makes runs first — the environment and token, the
/// storage home and its sweep, what this kernel can enforce — so a
/// misconfigured host fails at boot. Only the agent engine is missing, and the
/// workstream that builds one composes `afr_supervisor::run` here.
fn run() -> ExitCode {
    install_logs(&ProcessEnv);
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let booted =
        Config::from_env(&ProcessEnv).and_then(|config| StorageHome::open(config.storage_home()));
    let home = match booted {
        Ok(home) => home,
        Err(error) => {
            let (code, reason, event) = (error.code().as_str(), error.to_string(), "run_failed");
            tracing::error!(error_code = code, reason, event);
            return ExitCode::FAILURE;
        }
    };
    let swept = home.sweep();
    let host = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
    if let Some(missing) = host.missing() {
        let event = EVENT_RUN_REFUSED;
        tracing::error!(
            error_code = code,
            missing,
            swept,
            event,
            "this host cannot build a sandbox"
        );
        return ExitCode::FAILURE;
    }
    let event = EVENT_RUN_REFUSED;
    tracing::error!(error_code = code, reason = NO_AGENT_ENGINE, swept, event);
    ExitCode::from(REFUSED)
}

/// Sends structured records to stderr at the level the environment names.
fn install_logs(env: &impl afd_core::env::EnvSource) {
    let level = afd_core::env::log_level(env, DEFAULT_LEVEL);
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(level)
        .init();
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
