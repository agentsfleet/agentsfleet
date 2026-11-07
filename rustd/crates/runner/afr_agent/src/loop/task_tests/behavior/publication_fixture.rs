//! Publication runs through the offered root tool and the executor/lease boundaries.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;

#[derive(Debug)]
pub(super) struct Publisher {
    pub(super) goals: Goals,
    pub(super) published: Mutex<Vec<TaskEffectInput>>,
}

impl Publisher {
    pub(super) fn receipt() -> TaskEffect {
        TaskEffect {
            effect_id: ITEM.to_owned(),
            ordinal: 1,
            state: TaskEffectState::Confirmed,
            expected_head: "a".repeat(40),
            tested_tree: "b".repeat(40),
            commit: Some("c".repeat(40)),
            pull_request_url: "https://github.com/example/project/pull/7".to_owned(),
        }
    }
}

#[async_trait::async_trait]
impl LeaseVerbs for Publisher {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn task_inputs(
        &self,
        query: TaskInputsQuery,
    ) -> Result<TaskInputsResponse<'static>, Unanswered> {
        let mut inputs = self.goals.task_inputs(query).await?;
        if !self
            .published
            .lock()
            .expect("fixture mutex is unpoisoned")
            .is_empty()
            && let Some(task) = inputs.current_task.as_mut()
        {
            task.effect_cursor = 1;
            task.confirmed_effect_watermark = 1;
        }
        Ok(inputs)
    }
    async fn task_effects(&self, query: TaskEffectsQuery) -> Result<TaskEffectsPage, Unanswered> {
        let published = !self
            .published
            .lock()
            .expect("fixture mutex is unpoisoned")
            .is_empty();
        Ok(TaskEffectsPage {
            effects: if published && query.ordinal_after == 0 {
                vec![Self::receipt()]
            } else {
                vec![]
            },
            next_cursor: None,
        })
    }
    async fn task_publish(&self, input: TaskEffectInput) -> Result<TaskEffect, Unanswered> {
        self.published
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(input);
        Ok(Self::receipt())
    }
    async fn task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        self.goals.task_decision(input).await
    }
}

pub(super) fn publisher() -> Publisher {
    let mut goals = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    if let Some(task) = goals.inputs.current_task.as_mut() {
        task.target = TaskTarget::ExistingPr {
            repository: "example/project".to_owned(),
            repository_id: 1,
            number: 7,
            branch: "repair".to_owned(),
            base: "main".to_owned(),
            head: "a".repeat(40),
            tree: "d".repeat(40),
            url: "https://github.com/example/project/pull/7".to_owned(),
            workflows: vec![],
        };
    }
    Publisher {
        goals,
        published: Mutex::new(vec![]),
    }
}

pub(super) fn candidate() -> String {
    serde_json::json!({ "tested_tree": "b".repeat(40), "changes": [{"path": "src/lib.rs", "contents": "cmVwYWlyCg=="}] }).to_string()
}

pub(super) async fn publish(
    script: &Script,
    native: &Publisher,
    executor: &afr_executor::ExecutorSlot<'_>,
) -> crate::Result<crate::RunOutput> {
    let tool = &afr_tools::catalog::EXEC_COMMAND;
    let engine = super::super::super::tests::engine(vec![Canned::boxed(tool, "unused")], script);
    let lease = assigned_lease(&[tool.name()], 16_384);
    let prepared = PreparedTask {
        execution_deadline: native.goals.inputs.execution_deadline,
        inputs: native.goals.inputs.clone(),
        allow_actions: true,
    };
    engine
        .run(AgentRun {
            lease: &lease,
            prepared_task: Some(&prepared),
            recovered_task: None,
            memory: afr_memory::Seed::default(),
            executor,
            mint: &af_egress::testing::CountingMint::never(),
            verbs: native,
            checkpoint: &crate::testing::Discard,
            context_checkpoints: &crate::testing::ContextMemory::default(),
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await
}
