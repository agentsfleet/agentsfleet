//! A goal check asks the task's own statements and the one constant
//! next-action question, and nothing a fleet's setup once authored.

use super::super::fixture::{Case, exercise};
use super::fixture::{Decisions, request};

// Value: protects=a goal decision is prepared from the task's stored questions
// plus the constant next action; fails_when=the questions are rebuilt from
// anything but the task, or the next action is dropped; seam=the production
// bind and reserve verbs over live Postgres, the reserved request read back.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_goal_check_uses_the_tasks_questions() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;

        let (issued, body) = fixture.reserve(&fixture.goal_reservation(&task)).await;

        assert!(issued, "a goal check is sent: {body}");
        let names: Vec<String> = request(&body)
            .questions
            .into_iter()
            .map(|question| question.name)
            .collect();
        assert_eq!(
            names,
            [
                "goal.defect-fixed",
                "goal.verification-current",
                "next_action"
            ]
        );
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
