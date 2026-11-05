//! Live task observations use the same daemon-owned order as their saved row.

use afd_wire::activity::{ActivityFrame, TaskActivityHistory};

pub(super) fn canonical<'a>(
    frames: &[ActivityFrame<'a>],
    history: &TaskActivityHistory,
) -> Vec<ActivityFrame<'a>> {
    frames
        .iter()
        .filter_map(|frame| {
            let Some(record) = frame.task_activity() else {
                return Some(frame.clone());
            };
            let input = record.progress();
            let saved = history
                .items
                .iter()
                .find(|saved| {
                    let progress = saved.progress();
                    std::mem::discriminant(*saved) == std::mem::discriminant(&record)
                        && progress.activity_id == input.activity_id
                        && progress.operation == input.operation
                        && progress.phase == input.phase
                })?
                .progress()
                .clone();
            Some(match frame {
                ActivityFrame::TaskDecisionStarted(_) => ActivityFrame::TaskDecisionStarted(saved),
                ActivityFrame::TaskDecisionCompleted(_) => {
                    ActivityFrame::TaskDecisionCompleted(saved)
                }
                ActivityFrame::TaskTransferProgress(_) => {
                    ActivityFrame::TaskTransferProgress(saved)
                }
                _ => return None,
            })
        })
        .collect()
}
