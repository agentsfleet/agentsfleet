//! The `agentsfleetd` entry point: the one place a status becomes an exit code.
//!
//! Everything worth testing lives in the library beside this file. What is left
//! here is what cannot be tested in-process — reading the real environment,
//! installing signal handlers, and setting a process status — which is why
//! `ExitCode` appears here and nowhere else in the crate.
//!
//! Every line below runs on EVERY invocation, including the one that parses no
//! subcommand. That is deliberate: the previous `main` carried the subcommand
//! match, two runtime constructions and the signal handler, and none of it
//! could be reached by a suite because a binary crate's internals cannot be
//! linked. Argument parsing, dispatch and the shutdown signal now live in
//! [`agentsfleetd::cli`] and [`agentsfleetd::signal`], where they can.

use std::process::ExitCode;

use afd_core::env::ProcessEnv;
use agentsfleetd::cli::{Cli, run};
use clap::Parser as _;

fn main() -> ExitCode {
    // `Cli::parse` exits the process itself on `--help`, `--version` and any
    // usage error — 2 for the last of those, which is what `serve_args.zig`'s
    // three error variants were reaching for and never managed: a usage error
    // is not a boot refusal, and an init system restarting on the latter should
    // not restart on the former.
    ExitCode::from(run(
        &Cli::parse(),
        &ProcessEnv,
        tokio::runtime::Runtime::new,
        agentsfleetd::signal::shutdown(),
    ))
}
