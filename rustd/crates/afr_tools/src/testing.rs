//! What the handler suites share: one call, made the way the router makes it.

use crate::lease::Lease;
use crate::runtime::{Tool, ToolContext, ToolOutput};

/// Calls `tool` with `arguments` from the supervisor, with `lease`'s state.
pub(crate) async fn call(
    tool: &dyn Tool,
    lease: &mut Lease<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.call(
        &arguments,
        ToolContext {
            executor: None,
            lease,
        },
    )
    .await
}
