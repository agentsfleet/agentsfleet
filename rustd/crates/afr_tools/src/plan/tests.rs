use serde_json::json;

use crate::handler::Typed;
use crate::lease::Lease;
use crate::plan::UpdatePlan;
use crate::runtime::ToolErrorCode;
use crate::testing::call;

#[tokio::test]
async fn test_update_plan_records_steps() {
    let tool = Typed::boxed(UpdatePlan);
    let plan = json!({
        "explanation": "the job log is a 302, so read the step summary first",
        "plan": [
            {"step": "Read the failed run", "status": "completed"},
            {"step": "Read the job's steps", "status": "in_progress"},
            {"step": "Check the deploy annotations", "status": "pending"},
        ],
    });

    let output = call(tool.as_ref(), &Lease::default(), plan).await;

    assert_eq!(output.error_code, None);
    assert_eq!(
        output.text,
        "the job log is a 302, so read the step summary first\n\
         [completed] Read the failed run\n\
         [in_progress] Read the job's steps\n\
         [pending] Check the deploy annotations"
    );
}

#[tokio::test]
async fn a_plan_without_an_explanation_lists_only_its_steps() {
    let tool = Typed::boxed(UpdatePlan);
    let plan = json!({"plan": [{"step": "Read the run", "status": "pending"}]});

    let output = call(tool.as_ref(), &Lease::default(), plan).await;

    assert_eq!(output.text, "[pending] Read the run");
}

#[tokio::test]
async fn a_step_with_an_unknown_status_is_refused() {
    let tool = Typed::boxed(UpdatePlan);
    let plan = json!({"plan": [{"step": "Read the run", "status": "blocked"}]});

    let output = call(tool.as_ref(), &Lease::default(), plan).await;

    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
}
