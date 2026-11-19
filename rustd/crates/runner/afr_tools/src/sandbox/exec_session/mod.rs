//! The suites of `exec_command` and `write_stdin`: `guffbolt`'s tool, run through this runner's
//! sandbox, egress guard and memory, so what agentsfleet's runs read is
//! proved here.

use guffbolt::tools::process::exec_session::{
    EMPTY_WRITE_YIELD_MIN, ExecCommand, INPUT_REFUSED, WRITE_UNDELIVERED, WriteStdin,
    YIELD_DEFAULT, YIELD_MAX, YIELD_MIN,
};

/// The session the scripted executor's process `id` is, as the sessions
/// name it.
fn session(id: afr_executor::ProcessId) -> guffbolt::tools::env::ProcessId {
    guffbolt::tools::env::ProcessId::new(id.get())
}

mod live_tests;
mod refusal_tests;
mod tests;
mod write_tests;
