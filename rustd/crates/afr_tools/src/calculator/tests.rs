use serde_json::json;

use crate::calculator::Calculator;
use crate::handler::Typed;
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::call;

async fn calculate(op: &str, values: serde_json::Value) -> ToolOutput {
    let tool = Typed::boxed(Calculator);
    call(tool.as_ref(), &mut Lease::default(), json!({"op": op, "values": values})).await
}

#[tokio::test]
async fn each_operation_answers_its_result() {
    for (op, values, answer) in [
        ("add", json!([1, 2, 3.5]), "6.5"),
        ("subtract", json!([10, 3, 2]), "5"),
        ("multiply", json!([2, 3, 4]), "24"),
        ("divide", json!([12, 2, 3]), "2"),
        ("power", json!([3, 4]), "81"),
        ("sqrt", json!([9]), "3"),
        ("min", json!([4, -1, 3]), "-1"),
        ("max", json!([4, -1, 3]), "4"),
        ("mean", json!([1, 2, 3, 4]), "2.5"),
        ("median", json!([3, 1, 2]), "2"),
        ("median", json!([4, 1, 3, 2]), "2.5"),
    ] {
        let output = calculate(op, values).await;
        assert_eq!(output.error_code, None, "{op}: {}", output.text);
        assert_eq!(output.text, answer, "{op}");
    }
}

#[tokio::test]
async fn a_wrong_count_of_values_is_refused_with_the_count_it_takes() {
    for (op, values, takes) in [
        ("power", json!([2]), "2..=2"),
        ("sqrt", json!([4, 9]), "1..=1"),
        ("subtract", json!([1]), "2..="),
        ("add", json!([]), "1..="),
    ] {
        let output = calculate(op, values).await;
        assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments), "{op}");
        assert!(output.text.contains(takes), "{op}: {}", output.text);
    }
}

#[tokio::test]
async fn a_result_that_is_not_finite_is_refused_rather_than_answered() {
    for (op, values) in [
        ("divide", json!([1, 0])),
        ("sqrt", json!([-1])),
        ("power", json!([10, 400])),
    ] {
        let output = calculate(op, values).await;
        assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments), "{op}");
        assert!(output.text.ends_with("not a finite number"), "{op}: {}", output.text);
    }
}
