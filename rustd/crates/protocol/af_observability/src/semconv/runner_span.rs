//! The runner's vocabulary: the lease span, its segments and the `GenAI`
//! spans under it.

use super::{
    ATTR_AGENT_ID, ATTR_EVENT_ID, ATTR_OPERATION_NAME, ATTR_PROVIDER_NAME, ATTR_REQUEST_MODEL,
    ATTR_USAGE_CACHE_CREATION_TOKENS, ATTR_USAGE_CACHE_READ_TOKENS, ATTR_USAGE_INPUT_TOKENS,
    ATTR_USAGE_OUTPUT_TOKENS,
};

/// The service, and the instrumentation scope, everything a runner emits
/// names: a span under it was opened on a runner host, never by the daemon.
pub const RUNNER_SCOPE_NAME: &str = "agentsfleet-runner";

/// The span one lease's work runs under; every runner span sits inside one.
pub const SPAN_RUNNER_LEASE: &str = "runner.lease";

/// Which runner a signal came from: the daemon's id for the runner's row, so
/// each runner in a farm is told apart and joins its daemon record.
pub const ATTR_RUNNER_ID: &str = "agentsfleet.runner.id";

/// The host a runner reported itself on.
pub const ATTR_RUNNER_HOST: &str = "agentsfleet.runner.host";

/// The lease a runner span belongs to.
pub const ATTR_LEASE_ID: &str = "agentsfleet.lease.id";

/// Whether a lease ran in the sandbox its fleet's previous lease left held,
/// rather than a fresh one; absent from a lease that used no sandbox.
pub const ATTR_SANDBOX_REUSED: &str = "agentsfleet.sandbox.reused";

/// The lease's memory hydrate, on the fallback path only.
pub const ATTR_RUNNER_MEMORY_MS: &str = "agentsfleet.runner.memory_ms";

/// The sandbox, from sizing through checkout.
pub const ATTR_RUNNER_SANDBOX_MS: &str = "agentsfleet.runner.sandbox_ms";

/// Inside the sandbox segment: the egress bind.
pub const ATTR_RUNNER_EGRESS_MS: &str = "agentsfleet.runner.egress_ms";

/// Inside the sandbox segment: a held sandbox thawed, or a fresh one built.
pub const ATTR_RUNNER_PREPARE_MS: &str = "agentsfleet.runner.prepare_ms";

/// Inside the sandbox segment: the bound repositories checked out.
pub const ATTR_RUNNER_CHECKOUT_MS: &str = "agentsfleet.runner.checkout_ms";

/// How long the turn's sandbox-side calls waited for the build.
pub const ATTR_RUNNER_SANDBOX_WAIT_MS: &str = "agentsfleet.runner.sandbox_wait_ms";

/// The model's turns, less the tools they called.
pub const ATTR_RUNNER_MODEL_MS: &str = "agentsfleet.runner.model_ms";

/// The tools the model's turns called.
pub const ATTR_RUNNER_TOOLS_MS: &str = "agentsfleet.runner.tools_ms";

/// The settle, from hold-or-destroy to the posted report.
pub const ATTR_RUNNER_REPORT_MS: &str = "agentsfleet.runner.report_ms";

/// Inside the model segment: the run's first answer chunk.
pub const ATTR_RUNNER_FIRST_TOKEN_MS: &str = "agentsfleet.runner.first_token_ms";

/// The lease span's segment keys, in the order the lease's last line logs them.
pub const RUNNER_SEGMENT_KEYS: [&str; 10] = [
    ATTR_RUNNER_MEMORY_MS,
    ATTR_RUNNER_SANDBOX_MS,
    ATTR_RUNNER_EGRESS_MS,
    ATTR_RUNNER_PREPARE_MS,
    ATTR_RUNNER_CHECKOUT_MS,
    ATTR_RUNNER_SANDBOX_WAIT_MS,
    ATTR_RUNNER_MODEL_MS,
    ATTR_RUNNER_TOOLS_MS,
    ATTR_RUNNER_REPORT_MS,
    ATTR_RUNNER_FIRST_TOKEN_MS,
];

/// One model turn.
pub const OPERATION_CHAT: &str = "chat";

/// One tool call.
pub const OPERATION_EXECUTE_TOOL: &str = "execute_tool";

/// The tool a call ran.
pub const ATTR_TOOL_NAME: &str = "gen_ai.tool.name";

/// The call's id within its run.
pub const ATTR_TOOL_CALL_ID: &str = "gen_ai.tool.call.id";

/// Every key the runner's spans carry: the lease span's identity, then the
/// `GenAI` spans under it.
pub const RUNNER_SPAN_KEYS: &[&str] = &[
    ATTR_RUNNER_ID,
    ATTR_RUNNER_HOST,
    ATTR_LEASE_ID,
    ATTR_SANDBOX_REUSED,
    ATTR_EVENT_ID,
    ATTR_AGENT_ID,
    ATTR_RUNNER_MEMORY_MS,
    ATTR_RUNNER_SANDBOX_MS,
    ATTR_RUNNER_EGRESS_MS,
    ATTR_RUNNER_PREPARE_MS,
    ATTR_RUNNER_CHECKOUT_MS,
    ATTR_RUNNER_SANDBOX_WAIT_MS,
    ATTR_RUNNER_MODEL_MS,
    ATTR_RUNNER_TOOLS_MS,
    ATTR_RUNNER_REPORT_MS,
    ATTR_RUNNER_FIRST_TOKEN_MS,
    ATTR_OPERATION_NAME,
    ATTR_PROVIDER_NAME,
    ATTR_REQUEST_MODEL,
    ATTR_USAGE_INPUT_TOKENS,
    ATTR_USAGE_OUTPUT_TOKENS,
    ATTR_USAGE_CACHE_READ_TOKENS,
    ATTR_USAGE_CACHE_CREATION_TOKENS,
    ATTR_TOOL_NAME,
    ATTR_TOOL_CALL_ID,
];
