#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test module: a missing key, element or tool should fail the test loudly"
)]

use serde_json::json;

use crate::backed::Backends;
use crate::run::Run;
use crate::runtime::ToolErrorCode;
use crate::testing::{call, no_sandbox};

/// The name of the tool these cases read: one whose arguments are a type.
const RECALL: &str = "memory_recall";

#[test]
fn a_typed_schema_is_one_flat_object_that_refuses_unknown_arguments() {
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);
    let tools = backends.tools(&[RECALL]).unwrap();
    let parameters = tools.tool(RECALL).unwrap().spec().parameters;

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
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);
    let tools = backends.tools(&[RECALL]).unwrap();
    let tool = tools.tool(RECALL).unwrap();

    for refused in [
        json!({"query": "k", "token": "smuggled"}),
        json!({"query": 7}),
        json!({"limit": 5}),
    ] {
        let output = call(tool, &run, refused.clone()).await;
        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments.into()),
            "{refused}"
        );
        assert!(
            output.text.starts_with("[invalid_arguments] "),
            "{}",
            output.text
        );
    }
    let parsed = call(tool, &run, json!({"query": "k"})).await;
    assert_eq!(parsed.text, "nothing remembered matches k");
    assert_eq!(parsed.error_code, None);
}
