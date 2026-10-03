//! Where a tool runs, the handler seam, and what a call hands back.

use std::fmt;

use afr_executor::Executor;

use crate::catalog::Entry;
use crate::schema::Schema;

/// Where a tool's handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// In the supervisor, beside the loop. Never touches the sandbox.
    Supervisor,
    /// Inside the lease's sandbox, through the executor connection.
    Sandbox,
    /// At the model provider, sent as the provider's own hosted tool spec.
    Provider,
}

/// What a handler is given for one call.
///
/// The router builds it per call and hands the executor only to a sandbox-side
/// handler, so a supervisor-side one cannot reach the sandbox by construction.
#[derive(Debug, Clone, Copy)]
pub struct ToolContext<'run> {
    /// The lease's executor; `None` for a supervisor-side call.
    pub executor: Option<&'run dyn Executor>,
}

/// Why a call failed, in the stable spelling the model and the thread read.
///
/// A fieldless vocabulary, kept a plain enum the way `afd_auth::Error` is
/// (`docs/RUST_ERROR_STANDARD.md` §"The shared hull"): it rides every refused
/// call's output, carries no cause, and is never boxed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorCode {
    /// The model called a tool this run was not offered.
    NotOffered,
    /// The tool is the provider's to run, and this provider offers none.
    HostedToolUnavailable,
    /// A sandbox-side tool was called on a run that has no sandbox.
    SandboxUnavailable,
}

impl ToolErrorCode {
    /// The spelling the output text carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotOffered => "tool_not_offered",
            Self::HostedToolUnavailable => "hosted_tool_unavailable",
            Self::SandboxUnavailable => "sandbox_unavailable",
        }
    }
}

impl fmt::Display for ToolErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one call hands back to the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// What the model reads back.
    pub text: String,
    /// The process's exit code, for a call that ran one.
    pub exit_code: Option<i32>,
    /// Why the call failed, for one that did.
    pub error_code: Option<ToolErrorCode>,
}

impl ToolOutput {
    /// A call that succeeded with `text`.
    #[must_use]
    pub fn succeeded(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit_code: None,
            error_code: None,
        }
    }

    /// A call that failed: the model reads `[code] detail`, so the code
    /// reaches it with the reason rather than as a bare string.
    #[must_use]
    pub fn failed(code: ToolErrorCode, detail: &str) -> Self {
        Self {
            text: format!("[{code}] {detail}"),
            exit_code: None,
            error_code: Some(code),
        }
    }
}

/// One tool's handler.
///
/// Its name and runtime come from the published [`Entry`] it serves, so a
/// handler cannot claim a runtime the catalog does not give its tool.
#[async_trait::async_trait]
pub trait Tool: Send + Sync + fmt::Debug {
    /// The published tool this handler serves.
    fn entry(&self) -> &'static Entry;

    /// What the model is told about the tool.
    fn schema(&self) -> &Schema;

    /// Runs one call. A failure the model caused, or one upstream, is an
    /// output with an error code: the run continues and the model reads why.
    async fn call(&self, arguments: &serde_json::Value, context: ToolContext<'_>) -> ToolOutput;

    /// The tool's name.
    fn name(&self) -> &'static str {
        self.entry().name()
    }

    /// Where the handler runs.
    fn runtime(&self) -> Runtime {
        self.entry().runtime()
    }
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
