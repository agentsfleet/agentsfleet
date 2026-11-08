//! Well-formed frozen lease data; native freeze itself has its own scenario.

use afd_admin::test_util::DecisionModelFixture;
use std::sync::Arc;

use af_core::{clock, etag::sha256_hex, id::Uuid7};
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::{
    decision::{DecisionAssignment, DecisionProfile, DecisionRequest},
    task::{
        DecisionAssessment, DecisionReceipt, DecisionReceiptInput, DecisionReceiptResult,
        DecisionReservationInput, DecisionTaskVersion, TaskLimits,
    },
};
use afd_db::test_util::mint_id;
use afd_fleet::task::{PreparedPlan, PreparedTask, TaskDependencies, TaskLifecycle as _, Tasks};
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::{Value, json, value::RawValue};
use sqlx::types::Json;

use super::super::fixture::{Case, GOALS, id};
pub(super) use super::reply::known;
use crate::report_seed::FIXTURE_KEK_HEX;
use crate::seed::{MODEL, PROVIDER};

pub(super) struct Decisions<'a> {
    pub case: &'a Case,
    pub tasks: Tasks,
    pub model: DecisionModelFixture,
    pub assignment: DecisionAssignment,
    pub revision: Uuid7,
    pub dependencies: TaskDependencies,
    pub vault: Vault,
}

impl<'a> Decisions<'a> {
    pub(super) async fn create(case: &'a Case) -> Self {
        Self::with_daily_budget(case, 1_000_000_000).await
    }

    pub(super) async fn with_daily_budget(case: &'a Case, daily: i64) -> Self {
        case.held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
        let model = DecisionModelFixture::create(&case.held.fixtures.database)
            .await
            .expect("reviewed decision model fixture");
        let assignment = assignment(case, &model).await;
        let config = json!({"name":"pull-request-repair", "x-agentsfleet":{"tools":[], "triggers":[],
            "budget":{"daily_nanos":daily.to_string()}, "decision_policy": policy()}});
        case.held
            .fixtures
            .native_config(&case.held.fleet, &config.to_string())
            .await;
        let mut connection = case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("frozen decision fixture");
        sqlx::query("UPDATE runner.leases SET decision_snapshot=$2 WHERE id=$1")
            .bind(&case.held.lease_key)
            .bind(Json(&assignment))
            .execute(&mut *connection)
            .await
            .expect("well-formed already-issued snapshot");
        let revision: Uuid7 =
            sqlx::query_scalar("SELECT head_revision_id FROM fleet.fleets WHERE id=$1")
                .bind(&case.held.fleet)
                .fetch_one(&mut *connection)
                .await
                .expect("accepted owner revision");
        drop(connection);
        let mut dependencies = TaskDependencies::from(&case.held.fixtures.plane());
        dependencies.admin_workspace = Some(case.scope.workspace_id.clone());
        let tasks = Tasks::new(
            case.held.leases.clone(),
            case.admissions.clone(),
            dependencies.clone(),
        );
        let vault = Vault::new(
            case.held.fixtures.database.clone(),
            Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("fixture KEK")),
            Entropy::new(),
        );
        let key = format!("native-decision-fixture-{}", case.scope.workspace_id);
        let raw =
            RawValue::from_string(json!({"api_key":key}).to_string()).expect("synthetic key body");
        vault
            .create(
                &case.scope.workspace_id,
                &SecretName::parse("typesafe").expect("fixed provider bag"),
                &SecretBody::parse(&raw).expect("valid secret object"),
                clock::now(),
            )
            .await
            .expect("platform key fixture");
        Self {
            case,
            tasks,
            model,
            assignment,
            revision,
            dependencies,
            vault,
        }
    }

    pub(super) async fn reprice(&self) {
        let mut details = self.model.model.details.clone();
        let price = details
            .decision
            .as_mut()
            .expect("native decision schedule")
            .prices
            .first_mut()
            .expect("base tier");
        price.input_nanos_per_mtok = 50_000_000_000;
        "https://example.test/reviewed-overage-prices".clone_into(&mut price.source_url);
        price.verified_at = clock::now();
        assert!(
            afd_admin::Models::update(
                &self.model.models,
                &self.model.model.id,
                details,
                clock::now(),
            )
            .await
            .expect("native reviewed price update")
        );
    }

    pub(super) fn reservation(
        &self,
        assessment: DecisionAssessment,
        task: Option<DecisionTaskVersion>,
    ) -> garde::Valid<DecisionReservationInput> {
        garde::Unvalidated::new(DecisionReservationInput {
            operation_id: mint_id().into(),
            fencing_token: self.case.lease().fencing_token,
            source_admission_id: self.case.held.event_id.clone().into(),
            assessment,
            task,
            retry_of: None,
            evidence: json!({"observation":"current head and checks"}),
        })
        .validate()
        .expect("complete bounded native request")
    }

    pub(super) async fn reserve(
        &self,
        input: &garde::Valid<DecisionReservationInput>,
    ) -> (bool, Value) {
        let response = self
            .tasks
            .reserve_decision(self.case.lease(), input)
            .await
            .expect("native reservation");
        assert!(!format!("{response:?}").contains("native-decision-fixture-"));
        let (issued, body) = response.into_parts();
        (issued, super::diagnostics::reservation(&body))
    }

    pub(super) async fn record(
        &self,
        call: &str,
        receipt: DecisionReceipt,
    ) -> DecisionReceiptResult {
        let input = garde::Unvalidated::new(DecisionReceiptInput {
            fencing_token: self.case.lease().fencing_token,
            receipt,
        })
        .validate()
        .expect("bounded receipt");
        self.tasks
            .record_decision(self.case.lease(), &id(call), &input)
            .await
            .expect("original-owner receipt")
    }

    pub(super) async fn bind_goal(&self) -> DecisionTaskVersion {
        self.bind_goal_with_limits(TaskLimits::default()).await
    }

    pub(super) async fn bind_goal_with_limits(&self, limits: TaskLimits) -> DecisionTaskVersion {
        let goal = GOALS.first().expect("accepted fixture objective");
        let questions = serde_json::from_value(json!([
            {"name":"goal.defect-fixed","question":{"type":"noul",
                "instructions":"Does the patch fix the task's accepted Linkwarden defect?",
                "criteria":{"true":"Current code and regression evidence demonstrate the accepted behavior.",
                    "false":"The defect remains, scope changed, or evidence is missing or contradictory."}}},
            {"name":"goal.verification-current","question":{"type":"noul",
                "instructions":"Does the required verification cover the exact proposed code?",
                "criteria":{"true":"Required checks passed for the current tested tree and pull-request head.",
                    "false":"A required check failed, is missing or pending, or covers older code."}}}
        ])).expect("independent complete accepted question fixture");
        let item = PreparedTask::from_parts(
            0..i32::try_from(goal.len()).expect("small source span"),
            (*goal).to_owned(),
            (*goal).to_owned(),
            questions,
            sha256_hex(goal.as_bytes()),
            json!({"repository":"example/linkwarden","repository_id":101,"installation_id":1,"number":101,
                "branch":"repair","base":"main","head":"0".repeat(40),"tree":"1".repeat(40),
                "url":"https://github.com/example/linkwarden/pull/101","workflows":[]}).to_string(),
            limits,
        );
        let plan = PreparedPlan::from_parts(
            self.case.held.event_id.clone(),
            self.revision.clone(),
            "accepted-decision-goal".to_owned(),
            vec![item],
        );
        let bindings = self
            .tasks
            .bind(self.case.lease(), &plan)
            .await
            .expect("native bound task");
        DecisionTaskVersion {
            task_id: bindings
                .items
                .first()
                .expect("one task")
                .task_id
                .as_ref()
                .expect("accepted goal owns a task")
                .to_string(),
            generation: 1,
        }
    }

    pub(super) async fn counts(&self) -> (i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT count(*) FROM runner.decision_calls WHERE original_lease_id=$1), (SELECT count(*) FROM runner.decision_payloads p JOIN runner.decision_calls c ON c.id=p.decision_call_id WHERE c.original_lease_id=$1), (SELECT count(*) FROM billing.usage_lines WHERE work_id=$1 AND seq >= $2)")
            .bind(&self.case.held.lease_key).bind(i64::from(af_wire::usage::DECISION_SEQ_BASE))
            .fetch_one(&mut *self.case.held.fixtures.database.acquire().await.expect("financial counts"))
            .await.expect("retained financial and private row counts")
    }

    pub(super) fn goal_reservation(
        &self,
        task: &DecisionTaskVersion,
    ) -> garde::Valid<DecisionReservationInput> {
        self.reservation(
            DecisionAssessment::Goal {
                action_group_id: mint_id().into(),
            },
            Some(task.clone()),
        )
    }

    pub(super) async fn call_snapshot(&self) -> (Value, Value) {
        sqlx::query_as("SELECT to_jsonb(c), to_jsonb(p) FROM runner.decision_calls c JOIN runner.decision_payloads p ON p.decision_call_id=c.id WHERE c.original_lease_id=$1")
            .bind(self.case.lease().lease_id)
            .fetch_one(&mut *self.case.held.fixtures.database.acquire().await.expect("retained assessment observer"))
            .await.expect("complete call and private payload")
    }
}

pub(super) async fn lock_slot(connection: &mut sqlx::PgConnection, case: &Case) -> i32 {
    let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .expect("owned blocker identity");
    sqlx::query("SELECT fleet_id FROM runner.fleet_slots WHERE fleet_id=$1 FOR UPDATE")
        .bind(&case.held.fleet)
        .execute(connection)
        .await
        .expect("existing issuance lock");
    pid
}

pub(super) fn policy() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../../../tests/fixtures/fleets/goal-repair/decision-policy.json"
    ))
    .expect("reviewed owner criteria fixture")
}

async fn assignment(case: &Case, model: &DecisionModelFixture) -> DecisionAssignment {
    let settings = model
        .model
        .details
        .decision
        .as_ref()
        .expect("reviewed model")
        .configuration
        .profile
        .clone();
    let revision_id: Uuid7 =
        sqlx::query_scalar("SELECT decision_revision_id FROM model.platform_models WHERE id=$1")
            .bind(&model.model.id)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("catalogue connection"),
            )
            .await
            .expect("immutable model revision");
    DecisionAssignment {
        revision_id: revision_id.into(),
        profile: DecisionProfile {
            model_id: model.model.id.to_string(),
            revision: settings.revision,
            api: settings.api,
            endpoint: settings.endpoint,
            model: model.model.model_name.clone(),
            accepted_models: settings.accepted_models,
            capabilities: settings.capabilities,
            input_max_bytes: settings.input_max_bytes,
        },
        intent: settings.intent,
        goal: settings.goal,
    }
}

pub(super) fn request(body: &Value) -> DecisionRequest {
    serde_json::from_value(
        body.pointer("/dispatch/request")
            .expect("physical send authorized")
            .clone(),
    )
    .expect("complete native decision request")
}
