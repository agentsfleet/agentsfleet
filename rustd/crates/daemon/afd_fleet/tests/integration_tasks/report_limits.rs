//! Financial settlement exhausts original limits without minting another admission.
use super::{
    fixture::{Case, exercise, id},
    report_settlement::request,
};
use af_core::id::Uuid7;
use af_wire::task::{DecisionTaskVersion, TaskDisposition, TaskLimits, TaskReport, TaskState};
use afd_fleet::task::{TaskLifecycle as _, TaskReportOwner as _};
use serde_json::Value;
use std::sync::Arc;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn measured_report_runtime_exhausts_original_budget_without_reissuing_or_double_charging() {
    exercise(async |case| {
        let task = bind(
            case,
            TaskLimits {
                running_seconds: 1,
                ..TaskLimits::default()
            },
        )
        .await;
        Box::pin(settle(case, task, None)).await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn final_lease_limit_refuses_continuation_before_missing_checkpoint_without_losing_payment() {
    exercise(async |case| {
        let task = bind(
            case,
            TaskLimits {
                leases: 1,
                ..TaskLimits::default()
            },
        )
        .await;
        Box::pin(settle(case, task, Some("lease_limit_reached"))).await;
    })
    .await;
}

async fn bind(case: &Case, limits: TaskLimits) -> DecisionTaskVersion {
    let plan = case.source_plan(case.held.event_id.clone(), "bounded-report", 1, limits);
    let result = case
        .tasks
        .bind(case.lease(), &plan)
        .await
        .expect("bounded accepted task");
    DecisionTaskVersion {
        task_id: result
            .items
            .first()
            .expect("one goal")
            .task_id
            .as_ref()
            .expect("native task")
            .to_string(),
        generation: 1,
    }
}

async fn settle(case: &Case, task: DecisionTaskVersion, reason: Option<&str>) {
    let semantic = TaskReport {
        task: task.clone(),
        disposition: TaskDisposition::Continue,
        decision_call_id: None,
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    };
    let report = request(case, Some(semantic));
    assert!(
        case.tasks
            .prepare_report(case.lease(), &report)
            .await
            .expect("current continuation evidence")
            .is_some()
    );
    let mut plane = case.held.fixtures.plane();
    plane.task_reports = Some(Arc::new(case.tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    if reason.is_none() {
        Box::pin(report_after_preparation_deadline(
            case, &plane, &report, now,
        ))
        .await;
    } else {
        plane
            .report(&case.held.runner, &report, now)
            .await
            .expect("financial settlement survives exhaustion");
    }
    let saved = exhausted(case, &task.task_id, reason).await;
    let retained = evidence(case, &id(&task.task_id), reason).await;
    assert_eq!(case.counts().await, (1, 1, 1));
    plane
        .report(&case.held.runner, &report, now)
        .await
        .expect("financial report replay");
    assert_eq!(
        case.tasks
            .read(&case.scope, &id(&task.task_id))
            .await
            .expect("unchanged exhausted task"),
        saved
    );
    assert_eq!(evidence(case, &id(&task.task_id), reason).await, retained);
    assert_eq!(case.counts().await, (1, 1, 1));
}

async fn evidence(case: &Case, task: &Uuid7, reason: Option<&str>) -> Value {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("accounting observer");
    let row: (Option<String>, bool, i64, bool) = sqlx::query_as("SELECT t.status_reason,t.terminal_at IS NOT NULL,(SELECT count(*) FROM billing.usage_lines WHERE work_id=$2),(SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id=$3) FROM fleet.tasks t WHERE t.id=$1")
        .bind(task).bind(&case.held.lease_key).bind(&case.held.event_id).fetch_one(&mut *connection).await.expect("committed task and original accounting");
    assert_eq!(row, (reason.map(str::to_owned), true, 1, true));
    sqlx::query_scalar("SELECT jsonb_build_object('event',(SELECT to_jsonb(e) FROM fleet.events e WHERE e.fleet_id=$1 AND e.event_id=$2),'charges',(SELECT jsonb_agg(to_jsonb(u) ORDER BY u.seq) FROM billing.usage_lines u WHERE u.work_id=$3),'lease',(SELECT to_jsonb(l) FROM runner.leases l WHERE l.id=$3))")
        .bind(&case.scope.fleet_id).bind(&case.held.event_id).bind(&case.held.lease_key)
        .fetch_one(&mut *connection).await.expect("original event charge and lease snapshot")
}

async fn exhausted(case: &Case, task: &str, reason: Option<&str>) -> afd_fleet::task::TaskSnapshot {
    let saved = case
        .tasks
        .read(&case.scope, &id(task))
        .await
        .expect("exhausted owner");
    assert_eq!(saved.context.state, TaskState::Exhausted);
    assert_eq!(
        (saved.context.generation, saved.context.stop_generation),
        (2, 1)
    );
    assert!(saved.context.usage.running_nanos > 0);
    if reason.is_none() {
        assert!(saved.context.usage.running_nanos >= 1_000_000_000);
    }
    saved
}

async fn report_after_preparation_deadline(
    case: &Case,
    plane: &afd_fleet::lease::LeaseService,
    report: &af_wire::report::ReportRequest<'_>,
    now: jiff::Timestamp,
) {
    use sqlx::Connection as _;
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("commit barrier");
    // The report statement locks the fleet's slot before it measures the
    // slice; holding the slot holds the report there (M245 §6).
    let mut held = connection.begin().await.expect("slot barrier");
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM runner.fleet_slots WHERE fleet_id=$1 FOR UPDATE",
    )
    .bind(&case.scope.fleet_id)
    .fetch_one(&mut *held)
    .await
    .expect("hold the report's slot row");
    let reporting = plane.report(&case.held.runner, report, now);
    let release = async {
        observe_report_commit(case, pid).await;
        // Preparation has finished and commit owns the original report. Advance
        // to its actual database deadline, without inventing a runtime cursor.
        sqlx::query("SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM lease_expires_at-clock_timestamp()))) FROM runner.leases WHERE id=$1")
            .bind(&case.held.lease_key).execute(&mut *held).await.expect("original task runtime reaches its native ceiling");
        held.commit()
            .await
            .expect("allow original report accounting");
    };
    let (result, ()) = Box::pin(tokio::time::timeout(
        std::time::Duration::from_secs(4),
        async { tokio::join!(reporting, release) },
    ))
    .await
    .expect("report deadline barrier is bounded");
    result.expect("original expired lease still settles measured work");
}

async fn observe_report_commit(case: &Case, pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("report lock observer"),
            )
            .await
            .expect("native commit slot lock wait");
            if blocked {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("report reaches commit after live preparation");
}
