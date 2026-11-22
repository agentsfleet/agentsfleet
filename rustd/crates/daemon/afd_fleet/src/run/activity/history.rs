//! Live task observations use the same daemon-owned order as their saved row.

use af_wire::activity::{ActivityFrame, DurableTaskActivityHistory, DurableTaskActivityRecord};

pub(super) fn canonical<'a>(
    frames: &[ActivityFrame<'a>],
    history: &DurableTaskActivityHistory,
) -> Vec<ActivityFrame<'a>> {
    frames
        .iter()
        .filter_map(|frame| {
            let Some(record) = frame.durable_task_activity() else {
                return Some(frame.clone());
            };
            let input = record.progress();
            let saved = history.items.iter().find(|saved| {
                let progress = saved.progress();
                std::mem::discriminant(*saved) == std::mem::discriminant(&record)
                    && progress.activity_id == input.activity_id
                    && progress.operation == input.operation
                    && progress.phase == input.phase
            })?;
            // The saved record is the frame's own kind, so its variant names
            // the frame it republishes as.
            Some(match saved.clone() {
                DurableTaskActivityRecord::DurableTaskDecisionStarted(saved) => {
                    ActivityFrame::DurableTaskDecisionStarted(saved)
                }
                DurableTaskActivityRecord::DurableTaskDecisionCompleted(saved) => {
                    ActivityFrame::DurableTaskDecisionCompleted(saved)
                }
                DurableTaskActivityRecord::DurableTaskTransferProgress(saved) => {
                    ActivityFrame::DurableTaskTransferProgress(saved)
                }
            })
        })
        .collect()
}
