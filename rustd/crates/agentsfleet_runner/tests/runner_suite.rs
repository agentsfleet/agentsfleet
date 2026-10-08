//! Every `agentsfleet_runner` test file, in one test binary.

#[path = "entries.rs"]
mod entries;
// `run --unsandboxed` exists in a debug build alone.
#[cfg(debug_assertions)]
#[path = "fake_daemon.rs"]
mod fake_daemon;
#[cfg(debug_assertions)]
#[path = "run.rs"]
mod run;
