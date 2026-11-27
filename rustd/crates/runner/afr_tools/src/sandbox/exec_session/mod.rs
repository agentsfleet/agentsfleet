//! The suites of `exec_command` and `write_stdin`: `guffbolt`'s tool, run through this runner's
//! sandbox, egress guard and memory, so what agentsfleet's runs read is
//! proved here.

use guffbolt::tools::process::exec_session::{
    EMPTY_WRITE_YIELD_MIN, INPUT_REFUSED, WRITE_UNDELIVERED, YIELD_DEFAULT,
};

/// The session the scripted guest agent's process `id` is, as the sessions
/// name it.
fn session(id: afr_guest_client::ProcessId) -> guffbolt::tools::env::ProcessId {
    guffbolt::tools::env::ProcessId::new(id.get())
}

mod live_tests;
mod refusal_tests;
mod tests;
mod write_tests;
