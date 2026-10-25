//! Child summaries stay private; root progress carries no generated notes.

use afd_context::{Compaction, Progress};
use afd_wire::{
    activity::ActivityFrame,
    context::{ContextProgress, ContextState},
};

use crate::engine::EventSink;

pub(super) struct Notifications<'a> {
    sink: &'a dyn EventSink,
    generation: u64,
    visible: bool,
    started: bool,
    finished: bool,
}

impl<'a> Notifications<'a> {
    pub(super) fn new(sink: &'a dyn EventSink, task: &Compaction, visible: bool) -> Self {
        Self {
            sink,
            generation: task.expected_generation.saturating_add(1),
            visible,
            started: false,
            finished: false,
        }
    }

    pub(super) fn observe(&mut self, progress: Progress) {
        match progress {
            Progress::Read(_) => {}
            Progress::Summarizing => {
                self.started = true;
                self.emit(ContextState::Summarizing);
            }
            Progress::Committed(generation) => {
                self.generation = generation;
                self.finished = true;
                self.emit(ContextState::Resuming);
            }
        }
    }

    fn emit(&self, state: ContextState) {
        if self.visible {
            self.sink
                .emit(ActivityFrame::ContextProgress(ContextProgress {
                    generation: self.generation,
                    state,
                }));
        }
    }
}

impl Drop for Notifications<'_> {
    fn drop(&mut self) {
        if self.started && !self.finished {
            self.emit(ContextState::Interrupted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Frames;

    fn task() -> Compaction {
        Compaction {
            identity: afd_context::Identity {
                scope: "private-branch".into(),
                model: "model".into(),
                pinned: "pinned".into(),
            },
            expected_generation: 4,
            prior_digest: String::new(),
            prior_summary: None,
            groups: Vec::new(),
            output_tokens: 1,
        }
    }

    #[test]
    fn dropped_root_summary_reports_interruption_but_unstarted_and_child_work_remain_silent() {
        let frames = Frames::default();
        let sink = frames.sink();
        for visible in [true, false] {
            let mut notices = Notifications::new(&sink, &task(), visible);
            notices.observe(Progress::Summarizing);
        }
        drop(Notifications::new(&sink, &task(), true));
        assert_eq!(
            frames.taken(),
            vec![
                ActivityFrame::ContextProgress(ContextProgress {
                    generation: 5,
                    state: ContextState::Summarizing
                }),
                ActivityFrame::ContextProgress(ContextProgress {
                    generation: 5,
                    state: ContextState::Interrupted
                }),
            ]
        );
    }
}
