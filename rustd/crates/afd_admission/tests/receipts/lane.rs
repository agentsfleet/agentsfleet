//! The row chain one admission needs, the fake queue it appends to, and the
//! log it leaves — one fleet per test, removed afterwards.

use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_db::{Db, DbRole};
use jiff::Timestamp;
use sqlx::{AssertSqlSafe, Connection as _};

/// The instant the seeded rows carry.
const SEEDED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The status that keeps every lease path off the fixture fleet.
const FLEET_STOPPED: &str = "stopped";

/// One fleet, stopped so no lease path picks it up, and the rows above it.
pub(crate) struct Lane {
    handle: TestDatabase,
    pub(super) database: Db,
    tenant: String,
    pub(super) workspace: String,
    pub(super) fleet: String,
}

impl Lane {
    /// Seeds a tenant, a workspace and a stopped fleet on the lane's database.
    pub(crate) async fn seed() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let seeded = Self {
            handle: lane,
            database,
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
        };
        let mut connection = seeded
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        TenantSeed::new(&seeded.tenant)
            .at(SEEDED_AT)
            .insert(&mut *connection)
            .await
            .expect("seeding a tenant");
        WorkspaceSeed::new(&seeded.workspace, &seeded.tenant)
            .at(SEEDED_AT)
            .insert(&mut *connection)
            .await
            .expect("seeding a workspace");
        drop(connection);
        seeded.seed_fleet().await;
        seeded
    }

    async fn seed_fleet(&self) {
        FleetSeed::new(&self.fleet, &self.workspace, &self.tenant, FLEET_STOPPED)
            .name("receipt-fixture-fleet")
            .at(SEEDED_AT)
            .native_setup()
            .insert(&mut *self.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("seeding a fleet");
    }

    /// The receipt this fleet's admission row carries, if any.
    pub(crate) async fn receipt(&self) -> Option<String> {
        sqlx::query_scalar("SELECT receipt FROM fleet.admissions WHERE fleet_id = $1::uuid")
            .bind(&self.fleet)
            .fetch_one(&mut *self.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("the fleet holds one admission row")
    }

    /// Stands in for the replay sweeper winning the race: every admission row
    /// inserted for THIS fleet gets `receipt` the moment it lands. The trigger
    /// and its function go when the returned guard drops.
    pub(crate) async fn sweep_first(&self, receipt: &str) -> Sweeper {
        let sweeper = Sweeper {
            url: self.handle.url(),
            name: format!("afd_it_sweep_{}", self.fleet.replace('-', "")),
        };
        let name = &sweeper.name;
        let mut admin = sqlx::PgConnection::connect(&sweeper.url)
            .await
            .expect("the lane's owner connects");
        // The names are this fixture's own minted ids and a constant receipt,
        // never input, which is what makes interpolating them safe. The guard
        // exists before the first statement, so a half-installed sweeper is
        // still removed.
        for statement in [
            format!(
                "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN UPDATE fleet.admissions SET receipt = '{receipt}' \
                 WHERE id = NEW.id; RETURN NULL; END $$"
            ),
            format!(
                "CREATE TRIGGER {name} AFTER INSERT ON fleet.admissions FOR EACH ROW \
                 WHEN (NEW.fleet_id = '{}'::uuid) EXECUTE FUNCTION public.{name}()",
                self.fleet
            ),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut admin)
                .await
                .expect("the stand-in sweeper installs");
        }
        sweeper
    }

    /// Stands in for a replay pass that claimed this fleet's rows and has not
    /// finished: every admission row inserted for THIS fleet carries a claim
    /// an hour out the moment it lands, so no pass in the shared lane can take
    /// it before the test does anything. Removed when the guard drops.
    pub(crate) async fn claim_on_insert(&self) -> Sweeper {
        let claimer = Sweeper {
            url: self.handle.url(),
            name: format!("afd_it_claim_{}", self.fleet.replace('-', "")),
        };
        let name = &claimer.name;
        let mut admin = sqlx::PgConnection::connect(&claimer.url)
            .await
            .expect("the lane's owner connects");
        // Both identifiers and the WHEN UUID come from this fixture's minted id.
        for statement in [
            format!(
                "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN NEW.replay_claim_expires_at := statement_timestamp() + interval '1 hour'; \
                 RETURN NEW; END $$"
            ),
            format!(
                "CREATE TRIGGER {name} BEFORE INSERT ON fleet.admissions FOR EACH ROW \
                 WHEN (NEW.fleet_id = '{}'::uuid) EXECUTE FUNCTION public.{name}()",
                self.fleet
            ),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut admin)
                .await
                .expect("the stand-in claim installs");
        }
        claimer
    }

    /// Holds this fleet's admission rows `FOR UPDATE` on a connection of its
    /// own until the returned connection runs `COMMIT` or drops.
    pub(crate) async fn hold_rows(&self) -> sqlx::PgConnection {
        let mut holder = sqlx::PgConnection::connect(&self.handle.url())
            .await
            .expect("the lane's owner connects");
        sqlx::query("BEGIN")
            .execute(&mut holder)
            .await
            .expect("the holder opens a transaction");
        sqlx::query("SELECT id FROM fleet.admissions WHERE fleet_id = $1::uuid FOR UPDATE")
            .bind(&self.fleet)
            .execute(&mut holder)
            .await
            .expect("the holder locks the rows");
        holder
    }

    /// Refuses only this fixture's receipt updates, after the admission committed.
    pub(super) async fn refuse_receipts(&self) -> Sweeper {
        let guard = Sweeper {
            url: self.handle.url(),
            name: format!("afd_it_receipt_{}", self.fleet.replace('-', "")),
        };
        let name = &guard.name;
        let mut admin = sqlx::PgConnection::connect(&guard.url)
            .await
            .expect("fault owner");
        // Both identifiers and the WHEN UUID come from this fixture's minted ID.
        for statement in [
            format!(
                "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
                BEGIN RAISE EXCEPTION 'receipt fixture refusal'; END $$"
            ),
            format!(
                "CREATE TRIGGER {name} BEFORE UPDATE ON fleet.admissions FOR EACH ROW \
                WHEN (NEW.fleet_id='{}'::uuid AND NEW.receipt IS NOT NULL) \
                EXECUTE FUNCTION public.{name}()",
                self.fleet
            ),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut admin)
                .await
                .expect("install receipt refusal");
        }
        guard
    }

    /// Removes the fleet's rows, parents last.
    pub(crate) async fn cleanup(self) {
        for (table, id) in [
            ("DELETE FROM fleet.fleets WHERE id = $1::uuid", &self.fleet),
            (
                "DELETE FROM identity.workspaces WHERE id = $1::uuid",
                &self.workspace,
            ),
            (
                "DELETE FROM identity.tenants WHERE id = $1::uuid",
                &self.tenant,
            ),
        ] {
            let _removed = sqlx::query(table)
                .bind(id)
                .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
                .await;
        }
        self.handle.cleanup().await;
    }
}

/// The event a sweeper that could not be removed is logged under.
const SWEEPER_NOT_REMOVED: &str = "test_sweeper_not_removed";

/// The stand-in sweeper's trigger and function on `fleet.admissions`,
/// removed when this guard drops — on success, or while a failed test unwinds,
/// because a drop runs then too. A trigger left behind would stamp its receipt
/// on nothing (it names one fleet), but it would outlive the run in a shared
/// schema.
#[must_use = "the trigger is removed when the guard drops, so bind it for the test's life"]
pub(crate) struct Sweeper {
    url: String,
    name: String,
}

impl Drop for Sweeper {
    fn drop(&mut self) {
        // A thread of its own, with a runtime of its own: a drop cannot await,
        // and blocking the test's runtime on its own connection would hang a
        // current-thread test.
        let (url, name) = (self.url.clone(), self.name.clone());
        match std::thread::spawn(move || remove(&url, &name)).join() {
            Ok(Ok(())) => {}
            Ok(Err(cause)) => tracing::warn!(
                event = SWEEPER_NOT_REMOVED,
                trigger = %self.name,
                error = %cause,
                "the stand-in sweeper could not be removed; reset the lane"
            ),
            Err(_panicked) => tracing::warn!(
                event = SWEEPER_NOT_REMOVED,
                trigger = %self.name,
                "the stand-in sweeper's removal thread panicked; reset the lane"
            ),
        }
    }
}

/// Drops the trigger `name` and its function, on a connection of its own.
fn remove(url: &str, name: &str) -> sqlx::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the lane must start a removal runtime");
    runtime.block_on(async {
        let mut connection = sqlx::PgConnection::connect(url).await?;
        for statement in [
            format!("DROP TRIGGER IF EXISTS {name} ON fleet.admissions"),
            format!("DROP FUNCTION IF EXISTS public.{name}()"),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut connection)
                .await?;
        }
        Ok(())
    })
}
