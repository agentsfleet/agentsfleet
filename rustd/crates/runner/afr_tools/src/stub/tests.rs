//! A stub offers the schema derived for its argument type, never JSON written
//! beside it.

use guffbolt::Describe as _;
use serde_json::json;

use super::{NoArguments, Stub};
use crate::catalog::UPDATE_PLAN;
use guffbolt::tools::Schema;

#[test]
fn test_stub_schema_is_derived() {
    let stub = Stub::new(&UPDATE_PLAN);
    let derived = Schema::of::<NoArguments>(UPDATE_PLAN.name());

    assert_eq!(stub.spec(), derived.spec(UPDATE_PLAN.name()));

    // The derived shape is the one every provider reads: a flat object that
    // takes nothing and refuses whatever key a model invents.
    assert_eq!(
        stub.spec().parameters,
        &json!({
            "type": "object",
            "additionalProperties": false,
            "description": "Takes no arguments.",
        })
    );
}

/// A sandbox-side stub crosses into the sandbox it is given and answers its
/// name, refuses when the sandbox refuses its read, and with no sandbox
/// refuses as the router's own refusal would.
#[tokio::test]
async fn a_sandbox_side_stub_reads_through_the_sandbox_or_refuses_without_one() {
    use crate::catalog::FILE_READ;
    use crate::run::Run;
    use crate::runtime::{RunTool, ToolErrorCode};
    use crate::testing::{Live, call, call_in};

    let live = Live::start().await;
    let stub = Stub::boxed(&FILE_READ);
    let tool: &RunTool<'_> = stub.as_ref();
    let run = Run::default();

    let unread = call_in(tool, &live.client, &run, json!({})).await;
    std::fs::write(live.root.join(super::STUB_PATH), "x")
        .unwrap_or_else(|failure| unreachable!("the scratch workspace takes a file: {failure}"));
    let crossed = call_in(tool, &live.client, &run, json!({})).await;
    let refused = call(tool, &run, json!({})).await;

    assert_eq!(
        unread.error_code,
        Some(ToolErrorCode::SandboxUnavailable.into()),
        "a read the sandbox refuses is the stub's refusal: {}",
        unread.text
    );
    assert!(
        unread.text.contains("the workspace has no such file"),
        "{}",
        unread.text
    );
    assert_eq!(crossed.error_code, None);
    assert_eq!(crossed.text, FILE_READ.name());
    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::SandboxUnavailable.into())
    );
    assert_eq!(
        refused.text,
        format!("[sandbox_unavailable] {}", FILE_READ.name())
    );
    live.stop().await;
}
