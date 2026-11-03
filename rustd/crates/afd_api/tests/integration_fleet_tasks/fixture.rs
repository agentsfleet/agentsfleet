//! Valid native rows for task reads, over shared or isolated API-role pools.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use afd_auth::{credential::Presented, directory::Digest};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{
        ApiKeySeed, FleetSeed, OwnedRows, TenantSeed, TestDatabase, WorkspaceSeed, mint_id,
    },
};
use afd_wire::{
    decision::{DecisionQuestion, NamedQuestion, NoulCriteria},
    task::TaskState,
};
use sqlx::Acquire as _;

use crate::harness;

pub(super) const SUBJECT: &str = "user_live_task_operator";

pub(super) struct Tasks<'lane> {
    pub(super) base: Owner<'lane>,
    pub(super) ids: [String; 4],
}

impl<'lane> Tasks<'lane> {
    pub(super) async fn create(lane: &'lane TestDatabase) -> Self {
        let base = Owner::create(lane).await;
        let mut ids = std::array::from_fn(|_| mint_id());
        ids.sort();
        let fixture = Self { base, ids };
        fixture.seed().await;
        fixture
    }

    pub(super) fn collection(&self) -> String {
        format!(
            "/v1/workspaces/{}/fleets/{}/tasks",
            self.base.workspace, self.base.fleet
        )
    }

    pub(super) fn item(&self, index: usize) -> String {
        format!(
            "{}/{}",
            self.collection(),
            self.ids
                .get(index)
                .expect("fixture index names one of four tasks")
        )
    }

    pub(super) fn lifecycle(&self) -> afd_fleet::task::Tasks {
        let queue = afd_dragonfly::Dragonfly::unreachable(&harness::unreachable_queue())
            .expect("lazy task control queue");
        let leases = afd_fleet::lease::Leases::new(
            self.base.database.clone(),
            queue.clone(),
            Entropy::new(),
        );
        let admissions =
            afd_admission::Admissions::new(self.base.database.clone(), queue, Entropy::new());
        afd_fleet::task::Tasks::new(leases, admissions)
    }

    async fn seed(&self) {
        let questions = serde_json::to_string(&[NamedQuestion {
            name: "accepted_defect".into(),
            question: DecisionQuestion::Noul {
                instructions: "Does the patch fix the accepted defect?".into(),
                criteria: NoulCriteria {
                    yes: "Current regression evidence demonstrates the repair.".into(),
                    no: "The defect or missing evidence remains.".into(),
                },
            },
        }])
        .expect("typed questions serialize");
        let admission = mint_id();
        let mut connection = self.base.database.acquire().await.expect("API connection");
        let mut transaction = connection.begin().await.expect("task seed transaction");
        for (ordinal, (id, state)) in self
            .ids
            .iter()
            .zip([
                TaskState::Running,
                TaskState::Waiting,
                TaskState::Ready,
                TaskState::Done,
            ])
            .enumerate()
        {
            let item = mint_id();
            sqlx::query(include_str!("seed_item.sql"))
                .bind(&item)
                .bind(&self.base.fleet)
                .bind(self.base.workspace.as_str())
                .bind(&admission)
                .bind(i32::try_from(ordinal).expect("four fixture items"))
                .bind(id)
                .execute(&mut *transaction)
                .await
                .expect("immutable item");
            sqlx::query(include_str!("seed_task.sql"))
                .bind(id)
                .bind(&self.base.fleet)
                .bind(&item)
                .bind(format!("PR #{} repair", 123 + ordinal))
                .bind(&questions)
                .bind(state.as_str())
                .bind(state.is_terminal())
                .bind(if ordinal == 0 { 0_i64 } else { 1_000_i64 })
                .execute(&mut *transaction)
                .await
                .expect("task owner");
        }
        transaction
            .commit()
            .await
            .expect("task seed commits together");
    }
}

pub(super) struct Owner<'lane> {
    pub(super) lane: &'lane TestDatabase,
    pub(super) database: Db,
    tenant: String,
    pub(super) workspace: Uuid7,
    pub(super) fleet: String,
    pub(super) token: String,
    // Pooled handles drop first on unwind, before cleanup waits on their rows.
    cleanup: OwnedRows,
}

impl<'lane> Owner<'lane> {
    async fn create(lane: &'lane TestDatabase) -> Self {
        let database = lane.open(DbRole::Api, &[]).await;
        let tenant = mint_id();
        let workspace = Uuid7::parse(&mint_id()).expect("workspace identity");
        let fleet = mint_id();
        let mut cleanup = OwnedRows::new(lane);
        cleanup.record(include_str!("cleanup.sql"), &tenant);
        let token = format!(
            "agt_t{}",
            format!("{}{}", mint_id(), mint_id()).replace('-', "")
        );
        let digest = Digest::of(&Presented::new(&token).expect("test token"));
        let mut connection = database.acquire().await.expect("owner connection");
        TenantSeed::new(&tenant)
            .name("Task progress owner")
            .insert(&mut *connection)
            .await
            .expect("tenant seeds");
        WorkspaceSeed::new(workspace.as_str(), &tenant)
            .name("Task verification")
            .insert(&mut *connection)
            .await
            .expect("workspace seeds");
        ApiKeySeed::new(&mint_id(), &tenant, digest.as_str(), SUBJECT)
            .insert(&mut *connection)
            .await
            .expect("reader credential seeds");
        FleetSeed::new(
            &fleet,
            workspace.as_str(),
            &tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("Repair progress")
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("fleet seeds");
        drop(connection);
        Self {
            lane,
            database,
            tenant,
            workspace,
            fleet,
            token,
            cleanup,
        }
    }

    pub(super) async fn other_workspace(&self) -> Uuid7 {
        let id = Uuid7::parse(&mint_id()).expect("other workspace identity");
        let mut connection = self.database.acquire().await.expect("workspace connection");
        WorkspaceSeed::new(id.as_str(), &self.tenant)
            .name("Other task workspace")
            .insert(&mut *connection)
            .await
            .expect("other workspace seeds");
        id
    }

    pub(super) async fn empty_fleet(&self) -> String {
        let id = mint_id();
        let mut connection = self
            .database
            .acquire()
            .await
            .expect("empty fleet connection");
        FleetSeed::new(
            &id,
            self.workspace.as_str(),
            &self.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("Pull request review")
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("owned empty fleet seeds");
        id
    }

    pub(super) async fn cleanup(mut self) {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.cleanup.finish())
            .await
            .expect("owned row cleanup is bounded")
            .expect("owned fixture deletes");
        tokio::time::timeout(std::time::Duration::from_secs(5), self.database.close())
            .await
            .expect("fixture pool close is bounded");
    }
}
