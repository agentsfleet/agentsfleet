//! The sandbox's kernel lane: every proof that needs a real Linux kernel,
//! root, a usable KVM device and the toolbox image. It prints what may differ
//! on Firecracker, holds it to `afr_engine`'s conformance suite, then runs the
//! engine's own trials — boot, confinement, exhaustion, egress, the tools and
//! the supervisor on a real microVM — and the toolbox's, each reported under
//! its kind.
//!
//! `make test-runner-kernel` runs it; `cargo test` only builds it, so it never
//! runs by accident and never rots. It refuses to start — exit 1, every missing
//! prerequisite named — rather than skip: a lane that skips silently is a lane
//! that passed without proving anything.
#![expect(
    clippy::print_stderr,
    reason = "the kernel lane is an operator-run example; its refusals are terminal output"
)]
// The report prints only from the Linux-only modules, so elsewhere there is
// no stdout print to expect.
#![cfg_attr(
    target_os = "linux",
    expect(
        clippy::print_stdout,
        reason = "the kernel lane is an operator-run example; its report is terminal output"
    )
)]

#[cfg(target_os = "linux")]
mod budgets;
#[cfg(target_os = "linux")]
mod confinement;
#[cfg(target_os = "linux")]
mod delegation;
#[cfg(target_os = "linux")]
mod egress;
#[cfg(target_os = "linux")]
mod egress_closed;
#[cfg(target_os = "linux")]
mod egress_owned;
#[cfg(target_os = "linux")]
mod egress_refill;
#[cfg(target_os = "linux")]
mod exhaustion;
#[cfg(target_os = "linux")]
mod exhaustion_concurrent;
#[cfg(target_os = "linux")]
mod files;
#[cfg(target_os = "linux")]
mod filesystems;
#[cfg(target_os = "linux")]
mod firecracker;
#[cfg(target_os = "linux")]
mod forked_kill;
#[cfg(target_os = "linux")]
mod git;
#[cfg(target_os = "linux")]
mod lane;
#[cfg(target_os = "linux")]
mod microvm_pool;
#[cfg(target_os = "linux")]
mod pin_mapping;
#[cfg(target_os = "linux")]
mod pin_pause;
#[cfg(target_os = "linux")]
mod release;
#[cfg(target_os = "linux")]
mod run;
#[cfg(target_os = "linux")]
mod supervisor;
#[cfg(target_os = "linux")]
mod supervisor_checks;
#[cfg(target_os = "linux")]
mod supervisor_model;
#[cfg(target_os = "linux")]
mod supervisor_plane;
#[cfg(target_os = "linux")]
mod supervisor_recovery;
#[cfg(target_os = "linux")]
mod supervisor_trials;
#[cfg(target_os = "linux")]
mod toolbox;
#[cfg(target_os = "linux")]
mod toolbox_trials;
#[cfg(target_os = "linux")]
mod tools;
#[cfg(target_os = "linux")]
mod trials;
#[cfg(target_os = "linux")]
mod verify;

use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    install_logs();
    lane::main()
}

/// Writes every record to stderr through the runner's own filter, at the
/// level `AGENTSFLEET_LOG_LEVEL` names and info when unset, so a failing
/// trial's output carries what its engine logged: each microVM's console
/// line (`sandbox_console`) among it.
#[cfg(target_os = "linux")]
fn install_logs() {
    use tracing_subscriber::Layer as _;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let level = af_core::env::log_level(
        &af_core::env::ProcessEnv,
        tracing::level_filters::LevelFilter::INFO,
    );
    let records = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(af_providers::settings::log_filter(level));
    tracing_subscriber::registry().with(records).init();
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!(
        "the sandbox's kernel lane needs Linux; run `make test-runner-kernel` on a Linux host"
    );
    ExitCode::FAILURE
}
