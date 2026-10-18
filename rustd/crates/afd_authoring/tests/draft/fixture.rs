//! The production vault seals fixture credentials; no platform default is shared.

use super::{mint, seed};
use afd_authoring::{
    Access, Drafts,
    types::{CreateDraft, Source},
};
use afd_core::{clock, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{OwnedRows, TestDatabase},
};
use afd_observability::{Analytics, Recorded, Telemetry};
use serde_json::Value;
use sqlx::Connection as _;
use sqlx::pool::PoolConnection;
use std::sync::{Arc, LazyLock};

/// One recorder for every fixture in the suite.
///
/// A maintenance pass expires and finishes every lane's stale work, so a step
/// about one fixture can be reported through another fixture's `Drafts`. Every
/// fixture reports here, and [`Fixture::reported`] keeps the steps about its
/// own workspace, so a fixture sees its steps whichever pass reported them.
/// Test binaries run one at a time, so no other process reports about it.
static RECORDING: LazyLock<(Analytics, Recorded)> = LazyLock::new(Analytics::recording);

/// The property every authoring step names its workspace under.
const WORKSPACE: &str = "workspace_id";

pub(super) struct Fixture {
    pub(super) db: Db,
    pub(super) drafts: Drafts,
    /// The suite's [`RECORDING`], read only through [`Fixture::reported`].
    recorded: Recorded,
    pub(super) owner: Uuid7,
    pub(super) member: Uuid7,
    pub(super) workspace: Uuid7,
    pub(super) model: Uuid7,
    pub(super) other_model: Uuid7,
    pub(super) tenant: Uuid7,
    pub(super) member_tenant: Uuid7,
    pub(super) catalogue: [Uuid7; 2],
    pub(super) runner: Uuid7,
    cleanup: OwnedRows,
    lane: TestDatabase,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        Self::in_lane(TestDatabase::shared()).await
    }

    pub(super) async fn isolated() -> Self {
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        afd_db::Migrator::new()
            .run(&migrator)
            .await
            .expect("isolated platform schema");
        migrator.close().await;
        Self::in_lane(lane).await
    }

    async fn in_lane(lane: TestDatabase) -> Self {
        let db = lane.open(DbRole::Api, &[]).await;
        let (analytics, recorded) = RECORDING.clone();
        let mut fixture = Self {
            drafts: Drafts::new(db.clone(), Entropy::new(), ledger(&db))
                .with_analytics(analytics.clone()),
            recorded: recorded.clone(),
            db,
            cleanup: OwnedRows::new(&lane),
            lane,
            owner: mint(),
            member: mint(),
            workspace: mint(),
            model: mint(),
            other_model: mint(),
            tenant: mint(),
            member_tenant: mint(),
            catalogue: [mint(), mint()],
            runner: mint(),
        };
        fixture.arm_cleanup();
        seed::people(&fixture).await;
        seed::credential(&fixture).await;
        for (id, entry) in fixture
            .catalogue
            .iter()
            .zip([&fixture.model, &fixture.other_model])
        {
            seed::model(&fixture, id, entry).await;
        }
        fixture
    }

    pub(super) fn access(&self) -> Access<'_> {
        Access {
            workspace: &self.workspace,
            subject: self.owner.as_str(),
        }
    }

    /// The steps about this fixture's workspace reported so far under the
    /// funnel event `name`, in order, by any fixture's pass.
    pub(super) fn reported(&self, name: &str) -> Vec<Telemetry> {
        let events = self.recorded.events().into_iter();
        let mine = |event: &Telemetry| {
            let event = event.event();
            let workspace = event.properties().get(WORKSPACE).and_then(Value::as_str);
            workspace == Some(self.workspace.as_str())
        };
        events
            .filter(|event| event.name() == name && mine(event))
            .collect()
    }

    pub(super) fn member_access(&self) -> Access<'_> {
        Access {
            workspace: &self.workspace,
            subject: self.member.as_str(),
        }
    }

    pub(super) fn request(&self, operation: Uuid7, message: Option<&str>) -> CreateDraft {
        CreateDraft {
            source: Source::Definition {
                definition: Box::new(super::definition()),
            },
            authoring_model_ref: self.model.to_string(),
            message: message.map(str::to_owned),
            operation_id: operation,
        }
    }

    pub(super) async fn connection(&self) -> PoolConnection<sqlx::Postgres> {
        self.db.acquire().await.expect("fixture database answers")
    }

    /// The lane owner's URL, which a fault fixture installs its trigger as.
    pub(super) fn owner_url(&self) -> String {
        self.lane.url()
    }

    /// This workspace's drafts, candidates and messages, once no sweeper holds
    /// them.
    ///
    /// Every lane's maintenance pass sweeps every lane's expired drafts, and a
    /// pass skips rows another pass has locked. So the pass a test runs may
    /// leave its own draft to a concurrent one still purging it. Locking the
    /// workspace's draft rows first waits that purge out, and the counts after
    /// it read what it committed.
    pub(super) async fn counts(&self) -> (i64, i64, i64) {
        let mut connection = self.connection().await;
        let mut tx = connection.begin().await.expect("counts transaction");
        sqlx::query("SELECT id FROM authoring.drafts WHERE workspace_id=$1::uuid FOR UPDATE")
            .bind(self.workspace.as_str())
            .fetch_all(&mut *tx)
            .await
            .expect("wait out a concurrent purge");
        let counts = sqlx::query_as("SELECT (SELECT count(*) FROM authoring.drafts WHERE workspace_id=$1::uuid), \
            (SELECT count(*) FROM authoring.candidates c JOIN authoring.drafts d ON d.id=c.draft_id WHERE d.workspace_id=$1::uuid), \
            (SELECT count(*) FROM authoring.messages m JOIN authoring.drafts d ON d.id=m.draft_id WHERE d.workspace_id=$1::uuid)")
            .bind(self.workspace.as_str()).fetch_one(&mut *tx)
            .await.expect("scoped durable counts");
        tx.commit().await.expect("counts commit");
        counts
    }

    fn arm_cleanup(&mut self) {
        self.cleanup.record(
            "DELETE FROM authoring.replay_receipts WHERE workspace_id=$1::uuid",
            self.workspace.as_str(),
        );
        // Attempts carry no foreign key to the workspace (money outlives it),
        // so no cascade removes them; their dispatches, receipts and
        // settlements go with them.
        self.cleanup.record(
            "DELETE FROM authoring.attempts WHERE workspace_id=$1::uuid",
            self.workspace.as_str(),
        );
        self.cleanup.record(
            "DELETE FROM fleet.fleets WHERE tenant_id=$1::uuid",
            self.tenant.as_str(),
        );
        // A platform default holds its source workspace without a cascade, so
        // it goes before the tenant whose workspace it names.
        self.cleanup.record(
            "DELETE FROM model.platform_defaults WHERE source_workspace_id=$1::uuid",
            self.workspace.as_str(),
        );
        for tenant in [&self.tenant, &self.member_tenant] {
            self.cleanup.record(
                "DELETE FROM identity.tenants WHERE id=$1::uuid",
                tenant.as_str(),
            );
        }
        self.cleanup.record(
            "DELETE FROM runner.runners WHERE id=$1::uuid",
            self.runner.as_str(),
        );
        for id in &self.catalogue {
            self.cleanup.record(
                "DELETE FROM model.platform_models WHERE id=$1::uuid",
                id.as_str(),
            );
        }
    }

    pub(super) async fn cleanup(mut self) {
        self.cleanup
            .finish()
            .await
            .expect("remove only fixture-owned rows");
        assert_eq!(self.counts().await, (0, 0, 0), "private rows do not leak");
        self.db.close().await;
        self.lane.cleanup().await;
    }

    pub(super) fn own_library(&mut self, library: &str) {
        self.cleanup.record(
            "DELETE FROM authoring.library_entries WHERE id=$1::uuid",
            library,
        );
    }
}

pub(super) fn now() -> jiff::Timestamp {
    clock::now()
}

/// The built-in biller over the suite's database: setup chat reserves, logs and
/// settles through it exactly as the daemon's does.
pub(crate) fn ledger(db: &Db) -> Arc<dyn afd_billing::biller::Biller> {
    Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
        db.clone(),
    ))
}
