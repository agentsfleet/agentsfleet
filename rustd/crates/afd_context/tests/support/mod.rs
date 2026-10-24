use afd_context::{
    Checkpoint, CheckpointStore, Compaction, Group, HistorySource, Identity, Page, PortFuture,
    Result, Summarizer, Summary, TokenEstimator,
};

pub(super) fn identity() -> Identity {
    Identity {
        scope: "private-draft".into(),
        model: "selected-model-and-encoding".into(),
        pinned: "trusted-state".into(),
    }
}

pub(super) fn summary() -> Summary {
    Summary {
        goal: "Remember the original exact objective".into(),
        constraints: vec!["Do not publish without approval".into()],
        decisions: vec![],
        completed: vec![],
        remaining: vec!["Run the original acceptance test".into()],
        references: vec!["original-reference".into()],
    }
}

pub(super) struct Source {
    pub(super) groups: Vec<Group>,
    pub(super) reads: usize,
    pub(super) malformed: bool,
}

impl Source {
    pub(super) fn new(count: u64, bytes: usize) -> Self {
        Self {
            groups: (1..=count)
                .map(|sequence| Group {
                    sequence,
                    text: "x".repeat(bytes),
                })
                .collect(),
            reads: 0,
            malformed: false,
        }
    }
}

impl HistorySource for Source {
    fn page(&mut self, sequence: u64, limit: usize) -> PortFuture<'_, Page> {
        self.reads += 1;
        Box::pin(async move {
            let mut groups: Vec<_> = self
                .groups
                .iter()
                .filter(|group| group.sequence > sequence)
                .take(limit)
                .cloned()
                .collect();
            let has_more = self
                .groups
                .iter()
                .filter(|group| group.sequence > sequence)
                .count()
                > limit;
            if self.malformed {
                groups.reverse();
            }
            Ok(Page { groups, has_more })
        })
    }
}

pub(super) struct Estimator {
    pub(super) pinned: u64,
}

impl TokenEstimator for Estimator {
    fn continuation(&mut self, summary: Option<&Summary>, groups: &[Group]) -> Result<u64> {
        Ok(self.pinned
            + groups
                .iter()
                .map(|group| group.text.len() as u64)
                .sum::<u64>()
            + u64::from(summary.is_some()) * 100)
    }
    fn compaction(&mut self, task: &Compaction) -> Result<u64> {
        Ok(100
            + task
                .groups
                .iter()
                .map(|group| group.text.len() as u64)
                .sum::<u64>())
    }
}

#[derive(Default)]
pub(super) struct Store {
    pub(super) checkpoint: Option<Checkpoint>,
    pub(super) writes: usize,
    pub(super) refuse: bool,
}

impl CheckpointStore for Store {
    fn load(&mut self) -> PortFuture<'_, Option<Checkpoint>> {
        Box::pin(async { Ok(self.checkpoint.clone()) })
    }
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if self.refuse
                || expected_generation
                    != self.checkpoint.as_ref().map_or(0, |value| value.generation)
            {
                return Err(afd_context::Error::stale());
            }
            self.writes += 1;
            self.checkpoint = Some(checkpoint);
            Ok(())
        })
    }
}

pub(super) struct Model {
    pub(super) calls: usize,
    pub(super) refuse: bool,
}

impl Summarizer for Model {
    fn summarize<'a>(&'a mut self, _task: &'a Compaction) -> PortFuture<'a, Summary> {
        self.calls += 1;
        Box::pin(async move {
            if self.refuse {
                Err(afd_context::Error::summary())
            } else {
                Ok(summary())
            }
        })
    }
}
