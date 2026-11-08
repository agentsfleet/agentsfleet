#![expect(
    clippy::indexing_slicing,
    reason = "test module: a missing key or element should fail the test loudly"
)]

use serde_json::json;

use crate::handler::Typed;
use crate::lease::Lease;
use crate::memory::MemoryRecall;
use crate::runtime::ToolErrorCode;
use crate::testing::call;

#[test]
fn a_typed_schema_is_one_flat_object_that_refuses_unknown_arguments() {
    let tool = Typed::boxed(MemoryRecall);
    let parameters = tool.schema().parameters();

    assert_eq!(parameters["type"], "object");
    assert_eq!(parameters["additionalProperties"], false);
    assert_eq!(parameters["required"], json!(["query"]));
    assert_eq!(parameters["properties"]["query"]["type"], "string");
    for absent in ["$schema", "title", "$defs"] {
        assert!(parameters.get(absent).is_none(), "{absent} in {parameters}");
    }
}

#[tokio::test]
async fn arguments_that_do_not_parse_are_refused_before_the_handler_runs() {
    let tool = Typed::boxed(MemoryRecall);
    let lease = Lease::default();

    for refused in [
        json!({"query": "k", "token": "smuggled"}),
        json!({"query": 7}),
        json!({"limit": 5}),
    ] {
        let output = call(tool.as_ref(), &lease, refused.clone()).await;
        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{refused}"
        );
        assert!(
            output.text.starts_with("[invalid_arguments] "),
            "{}",
            output.text
        );
    }
    let parsed = call(tool.as_ref(), &lease, json!({"query": "k"})).await;
    assert_eq!(parsed.text, "nothing remembered matches k");
    assert_eq!(parsed.error_code, None);
}
