//! The sandbox's kernel lane: every proof that needs a real Linux kernel,
//! root, a usable KVM device and the toolbox image. It prints what may differ
//! on Firecracker, holds it to `afr_engine`'s conformance suite, then runs the
//! trials only a microVM answers and the toolbox's own, each reported under
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
mod firecracker;
#[cfg(target_os = "linux")]
mod git;
#[cfg(target_os = "linux")]
mod lane;
#[cfg(target_os = "linux")]
mod microvm_pool;
#[cfg(target_os = "linux")]
mod release;
#[cfg(target_os = "linux")]
mod run;
#[cfg(target_os = "linux")]
mod toolbox_trials;
#[cfg(target_os = "linux")]
mod trials;
#[cfg(target_os = "linux")]
mod verify;

use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    lane::main()
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!(
        "the sandbox's kernel lane needs Linux; run `make test-runner-kernel` on a Linux host"
    );
    ExitCode::FAILURE
}
