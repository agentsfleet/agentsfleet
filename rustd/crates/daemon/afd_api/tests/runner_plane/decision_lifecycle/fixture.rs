//! Registered runners issue a real lease and share the router's native secret key.

mod storage;

use std::borrow::Cow;

use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::{
    event::EventType,
    runner::{
        AssignedPolicy, CapabilityReport, HeartbeatRequest, NetworkPolicy, RegisterRequest,
        SandboxTier,
    },
    task::{DecisionAssessment, DecisionReservationInput},
};
use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_auth::scope::ScopeSet;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{OwnedRows, TestDatabase, mint_uuid7},
};
use afd_fleet::{
    lease::{Billed, Leases},
    task::Tasks,
};
use afd_runner::Runners;
use afd_vault::{Deleted, SecretBody, SecretName};
use axum::Router;
use serde_json::{json, value::RawValue};

use super::setup::{self, Owner};
use crate::harness;
use afd_admin::test_util::DecisionModelFixture;

pub(super) struct Runner {
    pub id: Uuid7,
    pub token: String,
}

pub(super) struct Lease {
    pub id: Uuid7,
    pub fence: u64,
    pub source: String,
}

pub(super) struct Fixture {
    pub database: Db,
    pub router: Router,
    pub owner: Owner,
    pub runner: Runner,
    pub other: Runner,
    pub lease: Lease,
    pub key: String,
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
        let mut model_cleanup = OwnedRows::new(&lane);
        let mut cleanup = OwnedRows::new(&lane);
        cleanup.record(
            "DELETE FROM runner.decision_payloads WHERE tenant_id = $1::uuid",
            owner.tenant.as_str(),
        );
        cleanup.record(
            "DELETE FROM fleet.task_checkpoints c USING identity.workspaces w WHERE c.workspace_id = w.id AND w.tenant_id = $1::uuid",
            owner.tenant.as_str(),
        );
        cleanup.record(include_str!("fixtures/cleanup.sql"), owner.tenant.as_str());
        cleanup.record(
            "DELETE FROM runner.runners WHERE labels ? $1::text",
            &owner.tag,
        );
        cleanup.record(
            "DELETE FROM model.platform_models WHERE id=$1::uuid",
            owner.coding_id.as_str(),
        );
        owner.seed(&database).await;
        let model = DecisionModelFixture::create(&database)
            .await
            .expect("reviewed decision model fixture");
        model_cleanup.record(
            "DELETE FROM model.platform_models WHERE id=$1::uuid",
            model.model.id.as_str(),
        );
        let [runner, other] = enroll(&database, &owner, &model.model.id).await;
        let queue = harness::connect_redis().await;
        let admissions = Admissions::for_tests(database.clone(), queue.clone());
        let leases = Leases::new(database.clone(), queue, Entropy::new());
        let lease = acquire(&leases, &admissions, &owner, &runner).await;
        let key = format!("decision-http-fixture-{}", mint_uuid7());
        save_key(&database, &owner.workspace, &key).await;
        let mut dependencies = harness::task_dependencies(database.clone());
        dependencies.admin_workspace = Some(owner.workspace.clone());
        dependencies.storage = Some(storage::build(&database, &owner).await);
        let tasks = Tasks::new(leases, admissions, dependencies);
        let router = harness::Fleet::live(
            database.clone(),
            "native_decision_http",
            ScopeSet::from_scopes(&[]),
        )
        .at(clock::now())
        .with_tasks(tasks)
        .router();
        Self {
            database,
            router,
            owner,
            runner,
            other,
            lease,
            key,
            model,
            cleanup,
            model_cleanup,
            lane,
        }
    }

    pub(super) fn collection(&self) -> String {
        format!("/v1/runners/me/leases/{}/tasks/decisions", self.lease.id)
    }

    pub(super) fn item(&self, call: &str) -> String {
        format!("{}/{call}", self.collection())
    }

    pub(super) fn reservation(&self) -> DecisionReservationInput {
        DecisionReservationInput {
            operation_id: mint_uuid7().to_string(),
            fencing_token: self.lease.fence,
            source_admission_id: self.lease.source.clone(),
            assessment: DecisionAssessment::Message,
            task: None,
            retry_of: None,
            evidence: json!({"checks":"not yet read"}),
        }
    }

    pub(super) async fn delete_key(&self) {
        assert_eq!(
            harness::vault(self.database.clone())
                .directory()
                .delete(
                    &self.owner.workspace,
                    &SecretName::parse("typesafe").expect("provider bag")
                )
                .await
                .expect("native secret deletion"),
            Deleted::Removed
        );
    }

    pub(super) async fn finish(mut self) {
        self.cleanup
            .finish()
            .await
            .expect("remove only owned retained and parent rows");
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

async fn enroll(database: &Db, owner: &Owner, model: &Uuid7) -> [Runner; 2] {
    let store = Runners::new(database.clone(), Entropy::new());
    let mut enrolled = Vec::new();
    for suffix in ["owner", "foreign"] {
        let registered = store
            .register(
                &RegisterRequest {
                    host_id: Cow::Owned(format!("{}-{suffix}", owner.tag)),
                    assigned_policy: AssignedPolicy {
                        durable_tasks_enabled: af_wire::runner::DURABLE_TASKS_DEFAULT_ENABLED,
                        decision_model_id: Some(Cow::Owned(model.to_string())),
                        sandbox_tier: SandboxTier::DevNone,
                        network_policy: NetworkPolicy::AllowAll,
                        registry_allowlist: Vec::new(),
                        worker_count: 1,
                        extra_binds: Vec::new(),
                    },
                    labels: vec![Cow::Borrowed(owner.tag.as_str())],
                },
                clock::now(),
            )
            .await
            .expect("native runner registration");
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

async fn acquire(
    leases: &Leases,
    admissions: &Admissions,
    owner: &Owner,
    runner: &Runner,
) -> Lease {
    let admitted = admit(admissions, owner).await;
    let acquired = leases
        .select(&runner.id, &[])
        .await
        .expect("native claim")
        .expect("owned work is runnable");
    assert_eq!(
        (&acquired.fleet_id, &acquired.event_id),
        (&owner.fleet, &admitted)
    );
    leases
        .record_received(&acquired)
        .await
        .expect("native run record");
    let issued = leases
        .issue(
            &runner.id,
            &acquired,
            Billed {
                tenant_id: &owner.tenant,
                posture: "platform",
                provider: setup::PROVIDER,
                model: &owner.coding_model,
            },
            clock::now(),
        )
        .await
        .expect("native issue")
        .expect("fenced lease still owned");
    assert!(issued.decision.is_some());
    Lease {
        id: issued.lease_id,
        fence: acquired.fence.as_u64(),
        source: admitted,
    }
}

async fn admit(admissions: &Admissions, owner: &Owner) -> String {
    let body = json!({"message":setup::MESSAGE}).to_string();
    let key = mint_uuid7().to_string();
    let admitted = admissions
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Repeated(&key),
            fleet: owner.fleet.as_str(),
            workspace: owner.workspace.as_str(),
            actor: &afd_events::steer_actor("native_decision_http"),
            event_type: EventType::Chat,
            request_json: &body,
            reply: Reply::None,
        })
        .await
        .expect("native admitted human source");
    admitted.stored.id
}

async fn save_key(database: &Db, workspace: &Uuid7, key: &str) {
    let raw =
        RawValue::from_string(json!({"api_key":key}).to_string()).expect("synthetic key object");
    harness::vault(database.clone())
        .create(
            workspace,
            &SecretName::parse("typesafe").expect("provider bag"),
            &SecretBody::parse(&raw).expect("valid key body"),
            clock::now(),
        )
        .await
        .expect("platform-admin secret");
}

async fn heartbeat(store: &Runners, runner: &Uuid7) {
    // Decision-enabled fleets require explicit durable-task support even
    // before their human admission has been bound to a task.
    let beat = store
        .heartbeat(
            runner,
            &HeartbeatRequest {
                capability_report: Some(CapabilityReport {
                    durable_tasks_v1: true,
                    build_id: None,
                    landlock: false,
                    seccomp: false,
                    cgroup_controllers: Vec::new(),
                    bubblewrap: false,
                    egress_enforcement: false,
                }),
                ..afd_runner::NO_REPORT
            },
            None,
            clock::now(),
        )
        .await
        .expect("native durable-task capability beat");
    assert_eq!(beat.verdict, afd_runner::Verdict::Healthy);
}
