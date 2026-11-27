//! The suites of the plain file tools: `guffbolt`'s tool, run through this runner's
//! sandbox, egress guard and memory, so what agentsfleet's runs read is
//! proved here.

use guffbolt::ToolOutput;

use crate::SandboxEnv;
use crate::sandbox::ScriptedGuest;

/// `path` through the file tools' gate, under the sandbox's `/workspace`.
fn inside(path: &str) -> Result<&str, ToolOutput> {
    let agent = ScriptedGuest::new([]);
    guffbolt::tools::files::inside(&SandboxEnv::new(&agent), path)
}

mod gate_tests;
mod limits_tests;
mod tests;
