//! Native recovery facts share the existing event activity and retained history.

use crate::client::ControlPlane;
use af_core::id::Uuid7;
use af_wire::activity::{
    ActivityFrame, ActivityRequest, DurableTaskActivity, DurableTaskActivityOperation,
    DurableTaskActivityPhase,
};

pub(super) async fn emit(
    plane: &ControlPlane,
    run: &Uuid7,
    task: &str,
    activity: &str,
    operation: DurableTaskActivityOperation,
    phase: DurableTaskActivityPhase,
    progress: Option<&super::progress::Progress>,
) {
    let request = ActivityRequest {
        frames: vec![ActivityFrame::DurableTaskTransferProgress(
            DurableTaskActivity {
                activity_id: activity.to_owned(),
                durable_task_id: Some(task.to_owned()),
                source_turn_id: None,
                operation,
                phase,
                provider: None,
                model: None,
                completed: progress.map_or(0, |progress| progress.completed),
                total: progress.and_then(|progress| progress.total),
                bytes: progress.map(|progress| progress.bytes),
                sequence: None,
            },
        )],
    };
    let _sent = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        plane.activity(run, &request),
    )
    .await;
}

impl super::Session<'_> {
    pub(super) async fn progress(
        &mut self,
        operation: DurableTaskActivityOperation,
        phase: DurableTaskActivityPhase,
        total: Option<u32>,
    ) {
        if let Some(total) = total {
            self.progress.total = Some(total);
        }
        if !self.progress.due(phase) {
            return;
        }
        emit(
            self.plane,
            self.run,
            &self.task.durable_task_id,
            &self.activity_id,
            operation,
            phase,
            Some(self.progress),
        )
        .await;
    }
}
