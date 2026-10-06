//! `agentsfleet-runner`: the one runner binary.
//!
//! Three entries, each only composing library crates:
//!
//! - `run` supervises this host's leases, as the systemd unit. It exports its
//!   spans and its own metric families to the runner collector when
//!   `OTEL_EXPORTER_OTLP_ENDPOINT` names one, and holds no credential to do it.
//! - `probe` prints what this host's kernel can enforce, as a heartbeat would
//!   carry it.
//! - `sandbox` is started by the engine inside each sandbox: it hardens itself
//!   before any thread exists, then serves the executor. The binary is bound
//!   read-only into every sandbox, so there is no second artifact to ship.

use std::process::ExitCode;

use afd_core::env::{EnvSource, ProcessEnv};
use afd_core::error_code;
use afr_telemetry::{Endpoint, SpanLayer, Telemetry};

use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Where a record goes when nobody chose.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Why `run` will not start in this build.
const NO_AGENT_ENGINE: &str = "this build carries no agent engine, so it takes no leases; the agent loop and its model providers arrive with their own runner workstream";

/// What `run` logs when it will not start.
const EVENT_RUN_REFUSED: &str = "run_refused";
/// The event a boot that could not read its configuration or open its storage
/// home is logged under.
const EVENT_RUN_FAILED: &str = "run_failed";

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

/// Starts the export when an endpoint is configured, boots, and delivers what
/// the export holds on the way out.
fn run() -> ExitCode {
    let telemetry = match exporting(&ProcessEnv) {
        Ok(telemetry) => telemetry,
        Err(refused) => return refused,
    };
    let ended = supervise();
    if let Some(telemetry) = telemetry {
        telemetry.close();
    }
    ended
}

/// Installs the subscriber, with the span export when the endpoint names a
/// collector, and says which.
///
/// Read before boot so a misconfigured export refuses the start rather than a
/// lease, and before the subscriber so the subscriber carries the export's
/// layer from its first record. A refusal is logged through a subscriber
/// without it, naming the knob and never its value.
fn exporting(env: &impl EnvSource) -> Result<Option<Telemetry>, ExitCode> {
    let resolved = Endpoint::from_env(env)
        .and_then(|endpoint| endpoint.as_ref().map(Telemetry::install).transpose());
    let telemetry = match resolved {
        Ok(telemetry) => telemetry,
        Err(refused) => {
            install_logs(env, None);
            let error_code = refused.code().as_str();
            let knob = refused.knob();
            let reason = refused.to_string();
            let event = EVENT_RUN_FAILED;
            tracing::error!(error_code, knob, reason, event);
            return Err(ExitCode::FAILURE);
        }
    };
    install_logs(env, telemetry.as_ref().map(Telemetry::layer));
    match &telemetry {
        Some(exporting) => exporting.announce(),
        None => afr_telemetry::announce_disabled(),
    }
    Ok(telemetry)
}

/// Boots as far as an agent engine is needed, then refuses.
///
/// Every check a real start makes runs first — the environment and token, the
/// storage home, what this kernel can enforce — so a misconfigured host fails
/// at boot. Only the agent engine is missing, and the workstream that builds
/// one composes `afr_supervisor::run` here, with the engine whose boot sweep
/// clears what a crashed runner left.
fn supervise() -> ExitCode {
    if let Err(error) = afr_supervisor::boot(&ProcessEnv) {
        let error_code = error.code().as_str();
        let reason = error.to_string();
        let event = EVENT_RUN_FAILED;
        tracing::error!(error_code, reason, event);
        return ExitCode::FAILURE;
    }
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let event = EVENT_RUN_REFUSED;
    let host = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
    if let Some(missing) = host.missing() {
        tracing::error!(
            error_code,
            missing,
            event,
            "this host cannot build a sandbox"
        );
        return ExitCode::FAILURE;
    }
    tracing::error!(error_code, reason = NO_AGENT_ENGINE, event);
    ExitCode::from(REFUSED)
}

/// Installs [`subscriber`] for the process, writing records to stderr.
fn install_logs(env: &impl EnvSource, spans: Option<SpanLayer>) {
    subscriber(env, spans, std::io::stderr).init();
}

/// Sends structured records to `records` through [`log_filter`], and the
/// runner's spans to `spans` when it exports.
///
/// The level filter sits on the record layer alone, so an operator quieting
/// the journal does not quiet the traces: the span layer carries a filter of
/// its own, admitting the runner's four span kinds and nothing else.
fn subscriber<W>(
    env: &impl EnvSource,
    spans: Option<SpanLayer>,
    records: W,
) -> impl tracing::Subscriber + Send + Sync + 'static
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let records = tracing_subscriber::fmt::layer()
        .with_writer(records)
        .with_filter(log_filter(env));
    tracing_subscriber::registry().with(spans).with(records)
}

/// The level the environment names, with the model library's own lines held
/// to its warnings, so no level puts a model's raw reply in the journal.
fn log_filter(env: &impl EnvSource) -> Targets {
    afr_providers::log_filter(afd_core::env::log_level(env, DEFAULT_LEVEL))
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
