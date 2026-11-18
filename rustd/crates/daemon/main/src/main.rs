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
//! [`agentsfleetd::cli`] and [`af_core::signal`], where they can.

#![deny(clippy::self_named_module_files)]

use std::process::ExitCode;

use af_core::env::ProcessEnv;
use agentsfleetd::cli::{Cli, run};
use clap::Parser as _;

fn main() -> ExitCode {
    // `Cli::parse` exits the process itself on `--help`, `--version` and any
    // usage error — 2 for the last of those, because a usage error is not a
    // boot refusal, and an init system restarting on the latter should not
    // restart on the former. It comes first here because it writes nothing
    // on the path that continues, which is what lets the nameplate still be the
    // process's first output.
    let cli = Cli::parse();

    // The first write of the process, before the subscriber, the runtime, or
    // any dependency is touched. Nothing it prints depends on any of them:
    // its only input is the version, which is why it can be this early.
    //
    // The width is not sensed. Reading a terminal's column count needs a
    // `TIOCGWINSZ` call that neither `std` nor any crate this workspace already
    // links provides, and the nameplate is not worth a dependency — so the
    // documented fallback governs and the rule is drawn at its full width.
    agentsfleetd::nameplate::show(
        env!("CARGO_PKG_VERSION"),
        &agentsfleetd::nameplate::Conditions::sense(),
        None,
        cli.quiet,
        cli.no_banner,
    );

    ExitCode::from(run(
        &cli,
        &ProcessEnv,
        tokio::runtime::Runtime::new,
        af_core::signal::shutdown(),
    ))
}
