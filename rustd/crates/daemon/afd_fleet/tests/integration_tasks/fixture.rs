//! Real lease/queue fixtures; only already-accepted plan data is synthetic.

use std::{panic::AssertUnwindSafe, time::Duration};

use af_core::{etag::sha256_hex, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::{
    decision::{DecisionQuestion, NamedQuestion, NoulCriteria},
    event::EventType,
    task::TaskLimits,
};
use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::test_util::TestDatabase;
use afd_fleet::task::{PreparedPlan, PreparedTask, TaskLease, TaskScope, Tasks};
use futures_util::FutureExt as _;
use serde_json::json;

use crate::{
    report_seed::{Held, held_admission},
    support::Fixtures,
};

pub(super) const GOALS: [&str; 3] = [
    "Repair title decoding in PR 101",
    "Prevent duplicate bookmarks in PR 102",
    "Reject empty tags in PR 103",
];

pub(super) struct Case {
    pub(super) held: Held,
    pub(super) tasks: Tasks,
    pub(super) scope: TaskScope,
    pub(super) revision: Uuid7,
    pub(super) revision_number: u64,
    pub(super) admissions: Admissions,
}

impl Case {
    async fn create(lane: &TestDatabase) -> Self {
        let actor = afd_events::steer_actor("native_task_owner");
        let request = json!({"message": GOALS.join("; ")}).to_string();
        let held = held_admission(
            Fixtures::create_in(lane.shared_handle()).await,
            &actor,
            &request,
        )
        .await;
        task_capability(&held, true).await;
        held.fixtures
            .native_config(
                &held.fleet,
                r#"{"name":"pull-request-repair","x-agentsfleet":{"tools":[],"triggers":[],"budget":{"daily_nanos":"1000000000"}}}"#,
            )
            .await;
        let (workspace, revision, number): (Uuid7, Uuid7, i64) = sqlx::query_as(
            "SELECT f.workspace_id,f.head_revision_id,r.revision FROM fleet.fleets f JOIN fleet.revisions r ON r.id=f.head_revision_id WHERE f.id=$1",
        )
        .bind(&held.fleet)
        .fetch_one(
            &mut *held
                .fixtures
                .database
                .acquire()
                .await
                .expect("scope connection"),
        )
        .await
        .expect("saved native revision");
        let scope = TaskScope {
            workspace_id: workspace,
            fleet_id: held.fleet.clone(),
        };
        let admissions = Admissions::new(
            held.fixtures.database.clone(),
            held.fixtures.bus().clone(),
            Entropy::new(),
        );
        let tasks = Tasks::new(
            held.leases.clone(),
            admissions.clone(),
            afd_fleet::task::TaskDependencies::from(&held.fixtures.plane()),
        );
        Self {
            held,
            tasks,
            scope,
            revision,
            revision_number: u64::try_from(number).expect("positive saved revision"),
            admissions,
        }
    }

    pub(super) const fn lease(&self) -> TaskLease<'_> {
        TaskLease {
            runner_id: &self.held.runner,
            lease_id: &self.held.lease_key,
            fencing_token: self.held.fence.as_u64(),
        }
    }

    pub(super) async fn sibling(&self) -> Self {
        Self::create(&self.held.fixtures.shared_lane()).await
    }

    pub(super) fn plan(&self, digest: &str, count: usize) -> PreparedPlan {
        self.source_plan(
            self.held.event_id.clone(),
            digest,
            count,
            TaskLimits::default(),
        )
    }

    pub(super) fn source_plan(
        &self,
        source: Uuid7,
        digest: &str,
        count: usize,
        limits: TaskLimits,
    ) -> PreparedPlan {
        PreparedPlan::from_parts(
            source,
            self.revision.clone(),
            digest.to_owned(),
            items(count, limits),
        )
    }

    /// The forthcoming input API owns this claim in production. Seeding it here
    /// exercises the existing binder's claim/fence validation without faking it.
    pub(super) async fn claimed_source(&self) -> Uuid7 {
        let actor = afd_events::steer_actor("native_task_owner");
        let source = self.source(Producer::Steer, &actor).await;
        self.claim(&source).await;
        source
    }

    pub(super) async fn source(&self, producer: Producer, actor: &str) -> Uuid7 {
        self.source_message(producer, actor, &GOALS.join("; "))
            .await
    }

    pub(super) async fn source_message(
        &self,
        producer: Producer,
        actor: &str,
        message: &str,
    ) -> Uuid7 {
        let key = afd_db::test_util::mint_id();
        let request = json!({"message": message}).to_string();
        let admission = self
            .admissions
            .admit(Admission {
                producer,
                key: Key::Repeated(key.as_str()),
                fleet: &self.scope.fleet_id,
                workspace: &self.scope.workspace_id,
                actor,
                event_type: EventType::Chat,
                request_json: &request,
                reply: Reply::None,
            })
            .await
            .expect("additional human source");
        admission.stored.id
    }

    pub(super) async fn claim(&self, source: &Uuid7) {
        sqlx::query(
            "UPDATE fleet.admissions SET input_claim_lease_id=$2,input_claim_fence=$3 WHERE id=$1",
        )
        .bind(source)
        .bind(&self.held.lease_key)
        .bind(self.held.fence.as_i64())
        .execute(
            &mut *self
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("claim connection"),
        )
        .await
        .expect("native input claim fixture");
    }

    pub(super) async fn counts(&self) -> (i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT count(*) FROM fleet.tasks WHERE fleet_id=$1), (SELECT count(*) FROM fleet.task_items WHERE fleet_id=$1), (SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1)")
            .bind(&self.scope.fleet_id)
            .fetch_one(&mut *self.held.fixtures.database.acquire().await.expect("count connection"))
            .await.expect("all residual state is readable")
    }
}

pub(super) fn items(count: usize, limits: TaskLimits) -> Vec<PreparedTask> {
    let message = GOALS.join("; ");
    GOALS
        .iter()
        .take(count)
        .map(|goal| {
            let start = message.find(goal).expect("accepted goal is in the message");
            PreparedTask::from_parts(
                i32::try_from(start).expect("small start")
                    ..i32::try_from(start + goal.len()).expect("small end"),
                (*goal).to_owned(),
                (*goal).to_owned(),
                questions(),
                sha256_hex(goal.as_bytes()),
                json!({"kind":"api_only"}).to_string(),
                limits,
            )
        })
        .collect()
}

pub(super) fn id(value: &str) -> Uuid7 {
    Uuid7::parse(value).expect("fixture identity is UUIDv7")
}

pub(super) fn questions() -> Vec<NamedQuestion> {
    vec![NamedQuestion {
        name: "accepted_regression".to_owned(),
        question: DecisionQuestion::Noul {
            instructions: "Does the unchanged regression pass at the repaired head?".to_owned(),
            criteria: NoulCriteria {
                yes: "The unchanged regression passes at the current head.".to_owned(),
                no: "It fails or current-head evidence is missing.".to_owned(),
            },
        },
    }]
}

pub(super) async fn exercise(scenario: impl AsyncFnOnce(&Case)) {
    af_core::test_util::trace::install_subscriber();
    let lane = tokio::time::timeout(Duration::from_secs(10), TestDatabase::create())
        .await
        .expect("owned database creation is bounded");
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(45), async {
        let case = Case::create(&lane).await;
        Box::pin(scenario(&case)).await;
        case.held.fixtures.cleanup().await;
    }))
    .catch_unwind()
    .await;
    tokio::time::timeout(Duration::from_secs(10), lane.cleanup())
        .await
        .expect("owned database cleanup is bounded after success or failure");
    match outcome {
        Ok(result) => result.expect("native lifecycle scenario is bounded"),
        Err(failure) => std::panic::resume_unwind(failure),
    }
}

/// Only this durable-task suite opts its native runner into the guarded protocol.
pub(super) async fn task_capability(held: &Held, enabled: bool) {
    use af_wire::runner::{HeartbeatRequest, HeldFleets};
    use std::borrow::Cow;

    for runner in [&held.runner, &held.spare] {
        let mut capability = crate::requests::capable();
        capability.durable_tasks_v1 = enabled;
        let holds = if runner == &held.runner {
            HeldFleets(vec![Cow::Borrowed(held.fleet.as_str())])
        } else {
            HeldFleets::default()
        };
        let request = HeartbeatRequest {
            capability_report: Some(capability),
            selftest: None,
            holds,
            closing: false,
        };
        let held_fleets = afd_runner::heartbeat::holds::prove(request.holds.clone());
        held.fixtures
            .runners()
            .heartbeat(
                runner,
                &request,
                held_fleets.as_ref(),
                jiff::Timestamp::now(),
            )
            .await
            .expect("task runner advertises its actual protocol support");
    }
}
