//! Structured lifecycle-event proofs for supervised tasks.

use af_core::test_util::trace::Capture;
use tracing::Level;

use super::*;

#[tokio::test]
async fn test_runtime_boundaries_emit_exactly_one_terminal_event() {
    const TASK: &str = "lifecycle_event_fixture";

    let recorder = Capture::install();
    let mut supervisor = Supervisor::new();
    supervisor.spawn(TASK, async |token| token.cancelled().await);
    let report = supervisor.shutdown().await;
    assert!(report.is_clean(), "the fixture task must stop cleanly");

    let events: Vec<_> = recorder
        .events()
        .into_iter()
        .filter(|record| record.fields.get("task").is_some_and(|task| task == TASK))
        .collect();
    assert_eq!(events.len(), 2, "one task must emit one pair: {events:?}");
    assert!(events.iter().all(|record| record.level == Level::INFO));

    let (Some(started_event), Some(terminal_event)) = (events.first(), events.last()) else {
        return;
    };
    let started = &started_event.fields;
    let terminal = &terminal_event.fields;
    assert_eq!(
        started.get("event").map(String::as_str),
        Some("supervised_task_started")
    );
    assert_eq!(
        terminal.get("event").map(String::as_str),
        Some("supervised_task_completed")
    );
    assert_eq!(started.get("task_id"), terminal.get("task_id"));
}

/// `[server] task_join_timeout_seconds` is the deadline a task that ignores
/// its token is abandoned at.
#[tokio::test(start_paused = true)]
async fn the_configured_join_timeout_abandons_a_task_that_will_not_stop() {
    let timeout = Duration::from_secs(3);
    let mut supervisor = Supervisor::new().with_join_timeout(timeout);
    assert_eq!(supervisor.join_timeout(), timeout);
    supervisor.spawn("ignores_its_token", async |_token| {
        tokio::time::sleep(Duration::from_secs(3_600)).await;
    });
    let started = tokio::time::Instant::now();
    let report = supervisor.shutdown().await;
    assert_eq!(report.abandoned, ["ignores_its_token"]);
    assert_eq!(started.elapsed(), timeout);
}
