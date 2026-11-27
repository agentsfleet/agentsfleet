//! A registered runner holds a real run on a person's message, served by the
//! router a live daemon builds, with a reviewed decision model and native
//! checkpoint custody behind it.

mod owner;
mod storage;

use std::borrow::Cow;

use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::{
    runner::{
        AssignedPolicy, CapabilityReport, HeartbeatRequest, NetworkPolicy, RegisterRequest,
        SandboxTier,
    },
    turn::TurnKind,
};
use afd_admin::test_util::DecisionModelFixture;
use afd_auth::scope::ScopeSet;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{OwnedRows, TestDatabase, mint_id},
};
use afd_fleet::{
    durable_task::DurableTasks,
    run::{Billed, Runs},
};
use afd_runner::Runners;
use afd_turns::{IncomingTurn, Key, Producer, Reply, Turns};
use afd_vault::{SecretBody, SecretName};
use axum::Router;
use serde_json::{json, value::RawValue};

pub(super) use owner::{BALANCE, MESSAGE, Owner};

use crate::harness;

/// The provider bag the decision model's key is read from.
const DECISION_PROVIDER_BAG: &str = "typesafe";

/// Tasks and decisions owned by the case's tenant, then the tenant itself.
const CLEANUP_TENANT: &str = "\
WITH decisions AS (DELETE FROM runner.decision_calls WHERE tenant_id = $1), \
charges AS (DELETE FROM billing.usage_lines WHERE tenant_id = $1) \
DELETE FROM identity.tenants WHERE id = $1";

pub(super) struct Runner {
    pub id: Uuid7,
    pub token: String,
}

pub(super) struct Run {
    pub id: Uuid7,
    pub fence: u64,
    pub source: Uuid7,
}

pub(super) struct Fixture {
    pub database: Db,
    pub router: Router,
    pub owner: Owner,
    pub runner: Runner,
    pub other: Runner,
    pub run: Run,
    pub key: String,
    pub tasks: DurableTasks,
    pub model: DecisionModelFixture,
    cleanup: OwnedRows,
    model_cleanup: OwnedRows,
    lane: TestDatabase,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let owner = Owner::new();
        let cleanup = owned(&lane, &owner);
        owner.seed(&database).await;
        let model = DecisionModelFixture::create(&database)
            .await
            .expect("reviewed decision model fixture");
        let mut model_cleanup = OwnedRows::new(&lane);
        model_cleanup.record(
            "DELETE FROM model.platform_models WHERE id=$1",
            &model.model.id,
        );
        let [runner, other] = enroll(&database, &owner, &model.model.id).await;
        let bus = harness::bus().await;
        let turns = Turns::for_tests(database.clone(), bus.clone());
        let runs = Runs::new(database.clone(), bus, Entropy::new());
        let run = acquire(&runs, &turns, &owner, &runner).await;
        let key = format!("durable-task-http-key-{}", mint_id());
        save_key(&database, &owner.workspace, &key).await;
        let mut dependencies = harness::durable_task_dependencies(database.clone());
        dependencies.admin_workspace = Some(owner.workspace.clone());
        dependencies.storage = Some(storage::build(&database, &owner).await);
        let tasks = DurableTasks::new(runs, turns, dependencies);
        let router = harness::Fleet::live(database.clone(), "durable_task_http", ScopeSet::EMPTY)
            .at(clock::now())
            .with_durable_tasks(tasks.clone())
            .router();
        Self {
            database,
            router,
            owner,
            runner,
            other,
            run,
            key,
            tasks,
            model,
            cleanup,
            model_cleanup,
            lane,
        }
    }

    pub(super) async fn finish(mut self) {
        self.cleanup
            .finish()
            .await
            .expect("remove only the case's rows");
        drop(self.router);
        self.model
            .finish()
            .await
            .expect("remove decision model fixture");
        self.model_cleanup
            .finish()
            .await
            .expect("disarm model unwind cleanup");
        self.database.close().await;
        self.lane.cleanup().await;
    }
}

/// The rows the case owns, removed whether it passes or panics.
fn owned(lane: &TestDatabase, owner: &Owner) -> OwnedRows {
    let mut cleanup = OwnedRows::new(lane);
    cleanup.record(
        "DELETE FROM runner.decision_payloads WHERE tenant_id = $1",
        &owner.tenant,
    );
    cleanup.record(
        "DELETE FROM runner.checkpoints c USING identity.workspaces w \
         WHERE c.workspace_id = w.id AND w.tenant_id = $1 AND c.durable_task_id IS NOT NULL",
        &owner.tenant,
    );
    cleanup.record(CLEANUP_TENANT, &owner.tenant);
    cleanup.record_text(
        "DELETE FROM runner.runners WHERE labels ? $1::text",
        &owner.tag,
    );
    cleanup.record(
        "DELETE FROM model.platform_models WHERE id=$1",
        &owner.coding_id,
    );
    cleanup
}

/// Two task-capable runners on the decision model: the run's, and another
/// that holds no run.
async fn enroll(database: &Db, owner: &Owner, model: &Uuid7) -> [Runner; 2] {
    let store = Runners::new(database.clone(), Entropy::new());
    let mut enrolled = Vec::new();
    for suffix in ["owner", "foreign"] {
        let policy = AssignedPolicy {
            durable_tasks_enabled: af_wire::test_util::DURABLE_TASKS_DEFAULT_ENABLED,
            decision_model_id: Some(Cow::Owned(model.to_string())),
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        };
        let request = RegisterRequest {
            host_id: Cow::Owned(format!("{}-{suffix}", owner.tag)),
            assigned_policy: policy,
            labels: vec![Cow::Borrowed(owner.tag.as_str())],
        };
        let registered = store
            .register(&request, "fixture:platform-operator", clock::now())
            .await
            .expect("runner registration");
        heartbeat(&store, &registered.runner_id).await;
        enrolled.push(Runner {
            id: registered.runner_id,
            token: registered.token.expose().to_owned(),
        });
    }
    enrolled
        .try_into()
        .ok()
        .expect("exactly two registered runners")
}

/// Admits the person's message and issues its run to `runner`.
async fn acquire(runs: &Runs, turns: &Turns, owner: &Owner, runner: &Runner) -> Run {
    let admitted = admit(turns, owner).await;
    let acquired = runs
        .select(&runner.id, &[])
        .await
        .expect("assignment")
        .expect("the person's message is runnable");
    assert_eq!(
        (&acquired.fleet_id, &acquired.turn_id),
        (&owner.fleet, &admitted)
    );
    let billed = Billed {
        tenant_id: &owner.tenant,
        posture: "platform",
        provider: owner::PROVIDER,
        model: &owner.coding_model,
    };
    let issued = runs
        .issue(&runner.id, &acquired, billed, clock::now())
        .await
        .expect("issue")
        .expect("the fenced run is still owned");
    assert!(
        issued.decision.is_some(),
        "the run carries the decision model"
    );
    Run {
        id: issued.run_id,
        fence: acquired.fence.as_u64(),
        source: admitted,
    }
}

async fn admit(turns: &Turns, owner: &Owner) -> Uuid7 {
    let body = json!({ "message": MESSAGE }).to_string();
    let key = mint_id();
    let admitted = turns
        .admit(IncomingTurn {
            producer: Producer::Steer,
            key: Key::Repeated(key.as_str()),
            fleet: &owner.fleet,
            workspace: &owner.workspace,
            actor: &afd_turns::view::steer_actor("durable_task_http"),
            event_type: TurnKind::Chat,
            request_json: &body,
            reply: Reply::None,
        })
        .await
        .expect("a person's message is admitted");
    admitted.stored.id
}

/// The decision model's provider key, in the workspace the daemon reads it
/// from.
async fn save_key(database: &Db, workspace: &Uuid7, key: &str) {
    let raw =
        RawValue::from_string(json!({ "api_key": key }).to_string()).expect("synthetic key object");
    harness::vault(database.clone())
        .create(
            workspace,
            &SecretName::parse(DECISION_PROVIDER_BAG).expect("provider bag"),
            &SecretBody::parse(&raw).expect("valid key body"),
            clock::now(),
        )
        .await
        .expect("decision provider key");
}

/// A capability beat that proves durable-task support, which a run on the
/// decision model requires before its person's message is bound to a task.
async fn heartbeat(store: &Runners, runner: &Uuid7) {
    let report = CapabilityReport {
        durable_tasks_v1: true,
        build_id: None,
        engine: af_wire::runner::SandboxEngine::Firecracker,
        isolation: af_wire::runner::Isolation::HardwareVm,
        kvm: af_wire::runner::Kvm::Usable,
        sandboxes_max: None,
    };
    let beat = store
        .heartbeat(
            runner,
            &HeartbeatRequest {
                capability_report: Some(report),
                ..afd_runner::NO_REPORT
            },
            None,
            clock::now(),
        )
        .await
        .expect("durable-task capability beat");
    assert_eq!(beat.verdict, afd_runner::Verdict::Healthy);
}
