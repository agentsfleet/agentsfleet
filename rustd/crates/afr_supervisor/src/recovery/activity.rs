//! Native recovery facts share the existing event activity and retained history.

use afd_core::id::Uuid7;
use afd_wire::activity::{ActivityFrame, ActivityRequest, TaskActivity, TaskActivityOperation, TaskActivityPhase};
use crate::client::ControlPlane;

pub(super) async fn emit(plane: &ControlPlane, lease: &Uuid7, task: &str, activity: &str,
    operation: TaskActivityOperation, phase: TaskActivityPhase, completed: u32, total: Option<u32>) {
    let request = ActivityRequest { frames: vec![ActivityFrame::TaskTransferProgress(TaskActivity {
        activity_id: activity.to_owned(), task_id: Some(task.to_owned()), source_admission_id: None,
        operation, phase, provider: None, model: None, completed, total,
    })] };
    let _sent = tokio::time::timeout(std::time::Duration::from_millis(500), plane.activity(lease, &request)).await;
}

impl super::Session<'_> {
    pub(super) async fn progress(&self, operation: TaskActivityOperation, phase: TaskActivityPhase, completed: u32, total: Option<u32>) {
        emit(self.plane, self.lease, &self.task.task_id, &self.activity_id, operation, phase, completed, total).await;
    }
}
