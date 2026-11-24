//! A task stopped at its last run owes its person a notice, even when the run
//! answered nothing.
use super::{fixture::exercise, report_limits::bind, report_settlement::request};
use af_wire::durable_task::{DurableTaskDisposition, DurableTaskLimits, DurableTaskReport};
use std::sync::Arc;

// Value: protects=a stopped task's notice reaches the person when its run
// answered nothing; fails_when=the notice only rewrites an answer already owed
// and a run with no text owes none; why_new=the report owes an answer inside
// its one statement, before the task settlement writes the notice;
// seam=none; no_claim=the notice's wording.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_notice_is_owed_when_the_run_answered_nothing() {
    exercise(async |case| {
        sqlx::query(
            "UPDATE thread.turns SET reply_provider = $2, reply_address = $3 WHERE id = $1",
        )
        .bind(&case.held.turn_id)
        .bind(afd_connector::Provider::Slack.id())
        .bind("C0FIXTURE:1700000000.000100")
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("producer"),
        )
        .await
        .expect("the turn asks for its answer on a thread");
        let task = bind(
            case,
            DurableTaskLimits {
                runs: 1,
                ..DurableTaskLimits::default()
            },
        )
        .await;
        let semantic = DurableTaskReport {
            durable_task: task,
            disposition: DurableTaskDisposition::Continue,
            decision_call_id: None,
            checkpoint_id: None,
            covered_input_sequence: 0,
            effect_watermark: 0,
        };
        let report = af_wire::report::ReportRequest {
            response_text: std::borrow::Cow::Borrowed(""),
            ..request(case, Some(semantic))
        };
        let mut plane = case.held.fixtures.plane();
        plane.durable_task_reports = Some(Arc::new(case.tasks.clone()));
        plane
            .report_with(
                &case.held.runner,
                &report,
                af_core::clock::after(case.held.now, crate::report_seed::SLICE),
                None,
                &mut af_observability::verb_span::VerbClock::untimed(),
            )
            .await
            .expect("the report settles");

        let (owed, answered): (Option<String>, String) = sqlx::query_as(
            "SELECT (SELECT o.answer FROM thread.deliveries o WHERE o.turn_id = e.id), \
                    e.answer_text FROM thread.turns e WHERE e.id = $1",
        )
        .bind(&case.held.turn_id)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("observer"),
        )
        .await
        .expect("the event reads");
        assert!(
            !answered.is_empty(),
            "the task's notice replaced the empty answer"
        );
        assert_eq!(
            owed.as_deref(),
            Some(answered.as_str()),
            "the notice is owed"
        );
    })
    .await;
}
