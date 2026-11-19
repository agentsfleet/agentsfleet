//! The suites of the plain file tools: `guffbolt`'s tool, run through this runner's
//! sandbox, egress guard and memory, so what agentsfleet's leases read is
//! proved here.

use guffbolt::ToolOutput;

use crate::SandboxEnv;
use crate::sandbox::ScriptedExecutor;

/// `path` through the file tools' gate, under the sandbox's `/workspace`.
fn inside(path: &str) -> Result<&str, ToolOutput> {
    let executor = ScriptedExecutor::new([]);
    guffbolt::tools::files::inside(&SandboxEnv::new(&executor), path)
}

mod gate_tests;
mod limits_tests;
mod tests;
