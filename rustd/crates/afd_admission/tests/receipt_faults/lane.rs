//! The row chain one admission needs, the fake queue it appends to, and the
//! log it leaves — one fleet per test, removed afterwards.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, DbRole};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_wire::event::EventType;
use sqlx::{AssertSqlSafe, Connection as _, Row as _};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// The instant the seeded rows carry.
const SEED_MS: i64 = 1_760_000_000_000;

/// Who the fixture's admission says raised it.
const ACTOR: &str = "fixture:receipt";

/// The body the fixture admits.
const REQUEST_JSON: &str = r#"{"message":"receipt fixture"}"#;

/// The field every event names its kind under.
const FIELD_EVENT: &str = "event";

/// One fleet, stopped so no lease path picks it up, and the rows above it.
pub(crate) struct Lane {
    handle: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    fleet: String,
}

/// One admission's outcome, what the fake was sent, and what was logged.
pub(crate) struct Run {
    pub(crate) outcome: afd_admission::Result<Admitted>,
    pub(crate) seen: Vec<String>,
    events: Vec<HashMap<String, String>>,
}

impl Run {
    /// The fields of the first event logged as `name`.
    pub(crate) fn event(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.events
            .iter()
            .find(|fields| fields.get(FIELD_EVENT).map(String::as_str) == Some(name))
    }
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
        for (statement, first, second) in [
            (
                "INSERT INTO core.tenants (id, name, created_at, updated_at) \
                 VALUES ($1::uuid, $1::text, $3, $3)",
                &seeded.tenant,
                &seeded.tenant,
            ),
            (
                "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
                 VALUES ($1::uuid, $2::uuid, $1::text, 'receipt-fixture', $3)",
                &seeded.workspace,
                &seeded.tenant,
            ),
        ] {
            seeded.execute(statement, first, second).await;
        }
        seeded.seed_fleet().await;
        seeded
    }

    async fn seed_fleet(&self) {
        sqlx::query(
            "INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, \
             config_json, status, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'receipt-fixture-fleet', '# fixture', \
                     '{}'::jsonb, 'stopped', $4, $4)",
        )
        .bind(&self.fleet)
        .bind(&self.workspace)
        .bind(&self.tenant)
        .bind(SEED_MS)
        .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
        .await
        .expect("seeding a fleet");
    }

    async fn execute(&self, statement: &'static str, first: &str, second: &str) {
        sqlx::query(statement)
            .bind(first)
            .bind(second)
            .bind(SEED_MS)
            .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("a seed row must insert");
    }

    /// The receipt this fleet's admission row carries, if any.
    pub(crate) async fn receipt(&self) -> Option<String> {
        sqlx::query("SELECT receipt FROM core.fleet_admissions WHERE fleet_id = $1::uuid")
            .bind(&self.fleet)
            .fetch_one(&mut *self.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("the fleet holds one admission row")
            .try_get(0)
            .expect("the receipt reads as text or null")
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
                 BEGIN UPDATE core.fleet_admissions SET receipt = '{receipt}' \
                 WHERE id = NEW.id; RETURN NULL; END $$"
            ),
            format!(
                "CREATE TRIGGER {name} AFTER INSERT ON core.fleet_admissions FOR EACH ROW \
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

    /// Removes the fleet's rows, parents last.
    pub(crate) async fn cleanup(self) {
        for (table, id) in [
            ("DELETE FROM core.fleets WHERE id = $1::uuid", &self.fleet),
            (
                "DELETE FROM core.workspaces WHERE id = $1::uuid",
                &self.workspace,
            ),
            ("DELETE FROM core.tenants WHERE id = $1::uuid", &self.tenant),
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

/// The stand-in sweeper's trigger and function on `core.fleet_admissions`,
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
            format!("DROP TRIGGER IF EXISTS {name} ON core.fleet_admissions"),
            format!("DROP FUNCTION IF EXISTS public.{name}()"),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut connection)
                .await?;
        }
        Ok(())
    })
}

/// Admits one event for `lane`'s fleet against a fake queue answering
/// `rules`, recording what the admission logged on this thread.
pub(crate) async fn admit_against(lane: &Lane, rules: &[(&str, Reply)]) -> Run {
    install_subscriber();
    let mut table = vec![("PING", Reply::Raw("+PONG\r\n"))];
    table.extend(rules.iter().cloned());
    let server = FakeRedis::spawn(&table).await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2));
    let queue = Dragonfly::connect(&config)
        .await
        .expect("a fake that answers PONG must be accepted");
    let admissions = Admissions::for_tests(lane.database.clone(), queue);
    let events = Arc::new(Mutex::new(Vec::new()));
    let outcome = {
        let _scoped = tracing::subscriber::set_default(
            tracing_subscriber::registry().with(Recorder(Arc::clone(&events))),
        );
        admissions
            .admit(Admission {
                producer: Producer::Steer,
                key: Key::Unrepeatable,
                fleet: &lane.fleet,
                workspace: &lane.workspace,
                actor: ACTOR,
                event_type: EventType::Chat,
                request_json: REQUEST_JSON,
                reply: ReplyTo::None,
            })
            .await
    };
    let events = events
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    Run {
        outcome,
        seen: server.seen(),
        events,
    }
}

/// Keeps every event's fields as text.
struct Recorder(Arc<Mutex<Vec<HashMap<String, String>>>>);

impl<S: tracing::Subscriber> Layer<S> for Recorder {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields(HashMap::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields.0);
    }
}

/// One event's fields, as text.
struct Fields(HashMap<String, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
