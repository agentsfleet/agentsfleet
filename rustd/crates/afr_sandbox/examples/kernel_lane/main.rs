//! The sandbox's kernel lane: every proof that needs a real Linux kernel,
//! root, bubblewrap and the toolbox image.
//!
//! `make test-runner-kernel` runs it; `cargo test` only builds it, so it never
//! runs by accident and never rots. It refuses to start — exit 1, every missing
//! prerequisite named — rather than skip: a lane that skips silently is a lane
//! that passed without proving anything.
//!
//! The same binary is the sandbox's entry: started as `kernel_lane sandbox`
//! inside bubblewrap, it hardens itself and serves the executor, exactly as
//! `agentsfleet-runner sandbox` does.

#[cfg(target_os = "linux")]
mod budgets;
#[cfg(target_os = "linux")]
mod confinement;
#[cfg(target_os = "linux")]
mod files;
#[cfg(target_os = "linux")]
mod git;
#[cfg(target_os = "linux")]
mod lane;
#[cfg(target_os = "linux")]
mod run;
#[cfg(target_os = "linux")]
mod toolbox;
#[cfg(target_os = "linux")]
mod tools;
#[cfg(target_os = "linux")]
mod trials;

use std::process::ExitCode;

/// The sub-command that hardens and serves inside the sandbox.
const SANDBOX: &str = afr_sandbox::bubblewrap::SANDBOX_SUBCOMMAND;

fn main() -> ExitCode {
    let sub = std::env::args().nth(1);
    if sub.as_deref() == Some(SANDBOX) {
        return serve();
    }
    lane()
}

#[cfg(target_os = "linux")]
fn serve() -> ExitCode {
    lane::serve()
}

#[cfg(target_os = "linux")]
fn lane() -> ExitCode {
    lane::main()
}

#[cfg(not(target_os = "linux"))]
fn serve() -> ExitCode {
    lane()
}

#[cfg(not(target_os = "linux"))]
fn lane() -> ExitCode {
    eprintln!(
        "the sandbox's kernel lane needs Linux; run `make test-runner-kernel` on a Linux host"
    );
    ExitCode::FAILURE
}
