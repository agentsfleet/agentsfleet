#![expect(
    clippy::indexing_slicing,
    reason = "test module: a missing key or element should fail the test loudly"
)]

use serde_json::json;

use crate::calculator::Calculator;
use crate::handler::Typed;
use crate::lease::Lease;
use crate::runtime::ToolErrorCode;
use crate::testing::call;

#[test]
fn a_typed_schema_is_one_flat_object_that_refuses_unknown_arguments() {
    let tool = Typed::boxed(Calculator);
    let parameters = &tool.schema().parameters;

    assert_eq!(parameters["type"], "object");
    assert_eq!(parameters["additionalProperties"], false);
    assert_eq!(parameters["required"], json!(["op", "values"]));
    assert_eq!(parameters["properties"]["op"]["enum"][0], "add");
    assert!(parameters["properties"]["op"].get("oneOf").is_none());
    for absent in ["$schema", "title", "$defs"] {
        assert!(parameters.get(absent).is_none(), "{absent} in {parameters}");
    }
}

#[tokio::test]
async fn arguments_that_do_not_parse_are_refused_before_the_handler_runs() {
    let tool = Typed::boxed(Calculator);
    let mut lease = Lease::default();

    for refused in [
        json!({"op": "add", "values": [1], "token": "smuggled"}),
        json!({"op": "add", "values": "one"}),
        json!({"op": "integrate", "values": [1]}),
        json!({"values": [1]}),
    ] {
        let output = call(tool.as_ref(), &mut lease, refused.clone()).await;
        assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments), "{refused}");
        assert!(output.text.starts_with("[invalid_arguments] "), "{}", output.text);
    }
    let parsed = call(tool.as_ref(), &mut lease, json!({"op": "add", "values": [1, 2]})).await;
    assert_eq!(parsed.text, "3");
    assert_eq!(parsed.error_code, None);
}

