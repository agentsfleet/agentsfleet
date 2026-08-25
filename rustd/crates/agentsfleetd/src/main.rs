//! The `agentsfleetd` entry point: the one place a refusal becomes an exit code.
//!
//! Everything worth testing lives in the library beside this file. What is left
//! here is what cannot be tested in-process — reading the real environment,
//! installing signal handlers, and setting a process status — which is why
//! `ExitCode` appears here and nowhere else in the crate, and why the fatal
//! renderer is CALLED here rather than being something a library reaches for.

use std::process::ExitCode;

use afd_core::env::ProcessEnv;
use agentsfleetd::{banner, fatal, migrate, preflight, serve};

/// The subcommand that boots and serves.
const SERVE: &str = "serve";

/// The subcommand that applies the schema and exits.
const MIGRATE: &str = "migrate";

fn main() -> ExitCode {
    let subcommand = std::env::args().nth(1);
    match subcommand.as_deref() {
        Some(SERVE) => serve_forever(),
        Some(MIGRATE) => migrate_once(),
        // No subcommand: preflight only. Useful as a container healthcheck and
        // as the thing an operator runs to find out what is missing before
        // taking the trouble to start anything.
        None => check_only(),
        Some(other) => {
            eprintln!(
                "agentsfleetd: unknown subcommand {other:?} (expected {SERVE:?} or {MIGRATE:?})"
            );
            // 2, not 1: a usage error is not a boot refusal, and an init system
            // restarting on the latter should not restart on the former.
            ExitCode::from(2)
        }
    }
}

/// Boots, serves until signalled, and reports how the background fleet stopped.
fn serve_forever() -> ExitCode {
    // The runtime is built here rather than by `#[tokio::main]` so that the
    // preflight refusal path never constructs one: a process that is about to
    // exit 1 over a missing knob has no use for a thread pool.
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            fatal::die(&error);
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async {
        match serve::run(&ProcessEnv, shutdown_signal()).await {
            Ok(outcome) => {
                if outcome.is_clean() {
                    ExitCode::SUCCESS
                } else {
                    // A task that would not stop, or a server that fell over.
                    // Reported as a failure so an operator sees it in the exit
                    // status and not only in a log line they have to go find.
                    eprintln!("agentsfleetd stopped unclean: {outcome:?}");
                    ExitCode::FAILURE
                }
            }
            Err(failure) => {
                fatal::die(&failure);
                ExitCode::FAILURE
            }
        }
    })
}

/// Applies the schema and exits, reporting which versions moved.
///
/// A separate runtime from `serve`'s, and a short-lived one: this is what a
/// release command or an init container runs, and it must not outlive the
/// migration by holding a pool open.
fn migrate_once() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            fatal::die(&error);
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async {
        match migrate(&ProcessEnv).await {
            Ok(applied) => {
                println!("agentsfleetd migrate — {}", migrate::summarise(&applied));
                ExitCode::SUCCESS
            }
            Err(failure) => {
                fatal::die(&failure);
                ExitCode::FAILURE
            }
        }
    })
}

/// Resolves the environment and reports it, without opening anything.
fn check_only() -> ExitCode {
    match preflight(&ProcessEnv) {
        Ok(config) => {
            let roles = [
                format!("postgres:{}", config.api_pool().role().tag()),
                format!("redis:{}", config.redis().role().tag()),
            ];
            banner::show(env!("CARGO_PKG_VERSION"), &roles, std::process::id());
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            fatal::die(&refusal);
            ExitCode::FAILURE
        }
    }
}

/// Resolves when the process is asked to stop.
///
/// Both signals, because they arrive from different places and mean the same
/// thing here: SIGTERM from an orchestrator, SIGINT from a terminal. `serve.zig`
/// watches both for the same reason.
async fn shutdown_signal() {
    let interrupt = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            // If SIGTERM cannot be registered, SIGINT alone still stops the
            // process. Refusing to boot over it would be worse than serving
            // with one of the two handlers.
            drop(interrupt.await);
            return;
        };
        tokio::select! {
            result = interrupt => drop(result),
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    drop(interrupt.await);
}
