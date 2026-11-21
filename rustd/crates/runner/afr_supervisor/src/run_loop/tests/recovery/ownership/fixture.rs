//! A native run, prepared task and sandbox build share one scoped recovery owner.

use super::*;

pub(super) struct Fixture {
    pub(super) rig: Rig,
    pub(super) run: RunPayload<'static>,
    pub(super) prepared: PreparedTask,
    key: HoldKey,
    egress: Egress,
    bound: Bound,
    stop: CancellationToken,
}

impl Fixture {
    pub(super) fn new(
        reply: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static,
    ) -> Result<Self> {
        let mut run = test_support::payload(test_support::RUN_ID, test_support::FLEET_ID);
        run.event.workspace_id = ID.into();
        let task = TaskContext {
            task_id: ID.into(),
            originating_item_id: ID.into(),
            target: TaskTarget::ApiOnly,
            generation: 7,
            stop_generation: 0,
            accepted_revision_id: ID.into(),
            objective: "Restore the accepted repair".into(),
            goal_digest: "b".repeat(64).into(),
            questions: vec![],
            state: af_wire::task::TaskState::Running,
            covered_input_sequence: 7,
            input_sequence: 7,
            effect_cursor: 5,
            confirmed_effect_watermark: 5,
            checkpoint_id: Some(ID.into()),
            checkpoint_generation: Some(1),
            expires_at: af_core::clock::after(
                af_core::clock::now(),
                std::time::Duration::from_mins(10),
            ),
            limits: af_wire::task::TaskLimits::default(),
            usage: af_wire::task::TaskUsage::default(),
        };
        run.durable_tasks = true;
        run.decision = Some(Box::new(
            crate::run_loop::tests::recovery::fixture::assignment(),
        ));
        run.run_expires_at = task.expires_at;
        run.task = Some(task.clone());
        Ok(Self {
            rig: Rig::new(
                test_support::daemon(reply),
                FakeEngine::default(),
                FakeAgent::new(Behaviour::Answer),
            ),
            key: HoldKey {
                fleet: Uuid7::parse(&run.event.fleet_id)?,
                workspace: ID.to_owned(),
                limits: Limits::default(),
                policy: BuiltUnder::allowing(&[]),
            },
            run,
            prepared: PreparedTask {
                execution_deadline: None,
                inputs: TaskInputsResponse {
                    source_turn_id: ID.to_owned(),
                    accepted_revision_id: Some(ID.to_owned()),
                    current_message: None,
                    inputs: vec![],
                    accepted_inputs: vec![],
                    current_interpretation: None,
                    current_task: Some(task),
                    tasks: vec![],
                    next_cursor: None,
                    execution_deadline: None,
                },
                allow_actions: true,
            },
            egress: test_support::assigned(af_wire::runner::NetworkPolicy::AllowAll, &[]),
            bound: Bound::Host,
            stop: CancellationToken::new(),
        })
    }

    pub(super) fn run(&self) -> Result<Run<'_>> {
        Ok(Run {
            lessee: &self.rig.lessee,
            payload: &self.run,
            egress: &self.egress,
            ids: Ids {
                run: Uuid7::parse(&self.run.run_id)?,
                fleet: self.key.fleet.clone(),
            },
            interrupt: self.stop.clone(),
            decisions: crate::verbs::PendingDecisions::default(),
            meter: Arc::new(Meter::new(Arc::clone(&self.rig.lessee.clock))),
            started: tokio::time::Instant::now(),
            timeline: Timeline::default(),
        })
    }

    pub(super) fn build<'a>(&'a self, recovery: &'a OnceLock<State>) -> Build<'a> {
        Build {
            prepared_task: Some(&self.prepared),
            recovery,
            key: &self.key,
            limits: Limits::default(),
            bound: &self.bound,
            plan: None,
            stop: &self.stop,
        }
    }
}

fn page() -> CheckpointPage {
    CheckpointPage {
        page_number: 0,
        entries: vec![],
        history_objects: vec![],
    }
}

fn plan() -> Result<CheckpointPlan> {
    let deadline = af_core::clock::after(af_core::clock::now(), std::time::Duration::from_mins(10));
    let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page()).map_err(error::encode)?);
    Ok(CheckpointPlan {
        checkpoint_id: ID.to_owned(),
        generation: 1,
        base_id: None,
        mode: CheckpointMode::Incremental,
        repository: "api-only".to_owned(),
        format_version: af_wire::task::CHECKPOINT_FORMAT,
        toolbox_digest: api_toolbox(),
        page_ids: vec![PAGE.to_owned()],
        base_page_ids: vec![],
        base_manifest_root: None,
        base_commit_oid: None,
        manifest_root: Some(af_crypto::checkpoint_digest::manifest_root([(
            PAGE,
            digest.as_str(),
        )])),
        history_digest: Some(af_crypto::checkpoint_digest::history_root(
            std::iter::empty::<(&str, &str, u64)>(),
        )),
        covered_input_sequence: 3,
        effect_watermark: 2,
        expires_at: deadline,
        pin: af_wire::task::CheckpointPin {
            id: ID.to_owned(),
            resource_version: 1,
            operation_deadline: deadline,
            expires_at: deadline,
        },
        replayed: false,
    })
}

pub(super) fn saved(call: &Call) -> Option<Answer> {
    match call.verb {
        Verb::TaskCheckpointAuthorizations => Some(match plan() {
            Ok(plan) => test_support::json(&CheckpointAuthorizations {
                plan,
                objects: vec![],
            }),
            Err(error) => Answer::Fail(error),
        }),
        Verb::TaskCheckpointPage => Some(test_support::json(&page())),
        _ => None,
    }
}
