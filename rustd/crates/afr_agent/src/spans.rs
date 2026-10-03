//! The spans a run is traced in, in OpenTelemetry's `GenAI` vocabulary and
//! under the runner's own scope, so a trace says which process opened each
//! one. They nest inside the supervisor's lease span, which names the runner.
//!
//! `invoke_agent` is the run, `chat` one model turn and `execute_tool` one
//! call. The keys are `afd_observability::semconv`'s, spelled nowhere else.

use afd_observability::semconv::{
    ATTR_OPERATION_NAME, ATTR_PROVIDER_NAME, ATTR_REQUEST_MODEL, ATTR_TOOL_CALL_ID, ATTR_TOOL_NAME,
    ATTR_USAGE_INPUT_TOKENS, ATTR_USAGE_OUTPUT_TOKENS, OPERATION_CHAT, OPERATION_EXECUTE_TOOL,
    OPERATION_INVOKE_AGENT, RUNNER_SCOPE_NAME, provider,
};
use afd_wire::policy::ExecutionPolicy;
use tracing::Span;
use tracing::field::Empty;

/// The span one run is traced in. A provider OpenTelemetry has no
/// well-known name for goes unnamed, never exported under a private word.
pub(crate) fn invoke_agent(policy: &ExecutionPolicy<'_>) -> Span {
    let vendor = provider::normalize(&policy.provider);
    let model = policy.context.model.as_ref();
    tracing::info_span!(
        target: RUNNER_SCOPE_NAME,
        OPERATION_INVOKE_AGENT,
        { ATTR_OPERATION_NAME } = OPERATION_INVOKE_AGENT,
        { ATTR_PROVIDER_NAME } = vendor,
        { ATTR_REQUEST_MODEL } = model,
    )
}

/// The span one model turn is traced in; its usage is recorded when the
/// turn ends.
pub(crate) fn chat(model: &str) -> Span {
    tracing::info_span!(
        target: RUNNER_SCOPE_NAME,
        OPERATION_CHAT,
        { ATTR_OPERATION_NAME } = OPERATION_CHAT,
        { ATTR_REQUEST_MODEL } = model,
        { ATTR_USAGE_INPUT_TOKENS } = Empty,
        { ATTR_USAGE_OUTPUT_TOKENS } = Empty,
    )
}

/// Records what a turn spent on its span.
pub(crate) fn spent(span: &Span, input: u64, output: u64) {
    span.record(ATTR_USAGE_INPUT_TOKENS, input);
    span.record(ATTR_USAGE_OUTPUT_TOKENS, output);
}

/// The span one tool call is traced in.
pub(crate) fn execute_tool(name: &str, call_id: &str) -> Span {
    tracing::info_span!(
        target: RUNNER_SCOPE_NAME,
        OPERATION_EXECUTE_TOOL,
        { ATTR_OPERATION_NAME } = OPERATION_EXECUTE_TOOL,
        { ATTR_TOOL_NAME } = name,
        { ATTR_TOOL_CALL_ID } = call_id,
    )
}
