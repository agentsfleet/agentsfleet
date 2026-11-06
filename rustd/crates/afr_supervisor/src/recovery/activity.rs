//! Native recovery facts share the existing event activity and retained history.

use crate::client::ControlPlane;
use afd_core::id::Uuid7;
use afd_wire::activity::{
    ActivityFrame, ActivityRequest, TaskActivity, TaskActivityOperation, TaskActivityPhase,
};

pub(super) async fn emit(
    plane: &ControlPlane,
    lease: &Uuid7,
    task: &str,
    activity: &str,
    operation: TaskActivityOperation,
    phase: TaskActivityPhase,
    progress: Option<&super::progress::Progress>,
) {
    let request = ActivityRequest {
        frames: vec![ActivityFrame::TaskTransferProgress(TaskActivity {
            activity_id: activity.to_owned(),
            task_id: Some(task.to_owned()),
            source_admission_id: None,
            operation,
            phase,
            provider: None,
            model: None,
            completed: progress.map_or(0, |progress| progress.completed),
            total: progress.and_then(|progress| progress.total),
            bytes: progress.map(|progress| progress.bytes.clone()),
            sequence: None,
        })],
    };
    let _sent = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        plane.activity(lease, &request),
    )
    .await;
}

impl super::Session<'_> {
    pub(super) async fn progress(
        &mut self,
        operation: TaskActivityOperation,
        phase: TaskActivityPhase,
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
            self.lease,
            &self.task.task_id,
            &self.activity_id,
            operation,
            phase,
            Some(self.progress),
        )
        .await;
    }
}
