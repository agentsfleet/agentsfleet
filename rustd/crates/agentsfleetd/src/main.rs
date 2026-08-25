//! The `agentsfleetd` entry point: the one place a refusal becomes an exit code.
//!
//! Everything worth testing lives in the library beside this file. What is left
//! here is what cannot be tested in-process — reading the real environment and
//! setting a process status — which is why `ExitCode` appears here and nowhere
//! else in the crate, and why the fatal renderer is CALLED here rather than
//! being something a library reaches for on its own.

use std::process::ExitCode;

use afd_core::env::ProcessEnv;
use agentsfleetd::{banner, fatal, preflight};

fn main() -> ExitCode {
    match preflight(&ProcessEnv) {
        Ok(config) => {
            // Boot proper arrives with the rest of §7. The roles are named
            // because a banner that cannot be wrong is a banner worth nothing:
            // this one reports what preflight actually resolved.
            // Qualified by datastore: both roles are named `api`, and a
            // banner reading "api · api" tells a reader nothing about which
            // two things came up.
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
