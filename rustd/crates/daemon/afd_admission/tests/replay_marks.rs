//! Retrying an admitted key republishes its hint without adding queue state.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;

use af_wire::event::EventType;
use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_bus::test_util::Server;
use afd_bus::{Handle, Received, Subject, Subscription};
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_db::{Db, DbRole};
use jiff::Timestamp;

use af_core::test_util::trace::{Capture, install_subscriber};

/// The instant the seeded rows carry.
const SEEDED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The status that keeps every lease path off the fixture fleet.
const FLEET_STOPPED: &str = "stopped";

/// The line a lost notification is logged under.
const RING_FAILED: &str = "admission_wake_ring_failed";

/// What the server half sends down a wake shard after the rings it holds.
const DRAINED: &str = "drained";

/// A stopped fleet and the rows above it, removed by [`Seeded::cleanup`].
struct Seeded {
    handle: TestDatabase,
    database: Db,
    tenant: Uuid7,
    workspace: Uuid7,
    fleet: Uuid7,
}

impl Seeded {
    async fn seed() -> Self {
        let handle = TestDatabase::shared();
        let database = handle.open(DbRole::Api, &[]).await;
        let seeded = Self {
            handle,
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
        FleetSeed::new(
            &seeded.fleet,
            &seeded.workspace,
            &seeded.tenant,
            FLEET_STOPPED,
        )
        .name("replay-fixture-fleet")
        .at(SEEDED_AT)
        .native_setup()
        .insert(
            &mut *seeded
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("seeding a fleet");
        seeded
    }

    /// Admits the one steer this suite resends, under `key`.
    async fn admit(&self, admissions: &Admissions, key: &str) -> Admitted {
        admissions
            .admit(Admission {
                producer: Producer::Steer,
                key: Key::Repeated(key),
                fleet: &self.fleet,
                workspace: &self.workspace,
                actor: "fixture:replay",
                event_type: EventType::Chat,
                request_json: r#"{"message":"replay fixture"}"#,
                reply: ReplyTo::None,
            })
            .await
            .expect("the ledger answers")
    }

    async fn cleanup(self) {
        for (statement, id) in [
            ("DELETE FROM fleet.fleets WHERE id = $1", &self.fleet),
            (
                "DELETE FROM identity.workspaces WHERE id = $1",
                &self.workspace,
            ),
            ("DELETE FROM identity.tenants WHERE id = $1", &self.tenant),
        ] {
            let _removed = sqlx::query(statement)
                .bind(id)
                .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
                .await;
        }
        self.handle.cleanup().await;
    }
}

/// A reader on every wake shard of `bus`, opened before anything rings.
async fn wake_readers(bus: &Handle) -> Vec<Subscription> {
    let mut readers = Vec::new();
    for shard in 0..afd_bus::wake::SHARDS {
        readers.push(
            bus.subscribe(&Subject::Wake(shard))
                .await
                .expect("an open in-process bus takes a reader"),
        );
    }
    readers
}

/// Every fleet rung on `readers` so far.
///
/// A reader sees one shard's messages in the order they were sent, so the
/// marker `server` sends last arrives after every ring already on that shard:
/// what comes before it is all the shard holds, with no deadline to guess.
async fn rung(server: &Server, readers: &mut [Subscription]) -> Vec<String> {
    let mut rung = Vec::new();
    for (shard, reader) in (0..afd_bus::wake::SHARDS).zip(readers.iter_mut()) {
        server.publish(&Subject::Wake(shard), DRAINED);
        loop {
            let received = reader
                .recv()
                .await
                .expect("an open in-process bus keeps its readers");
            let Received::Message(message) = received else {
                unreachable!("a reader a few rings deep cannot lag, got {received:?}");
            };
            if message.payload.as_str() == DRAINED {
                break;
            }
            rung.push(message.payload.as_str().to_owned());
        }
    }
    rung
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retry_republishes_a_lost_notification_for_the_same_admission() {
    install_subscriber();
    let fleet = Seeded::seed().await;
    let key = mint_id();

    let lost = Admissions::for_tests(fleet.database.clone(), afd_bus::test_util::unreachable());
    let capture = Capture::install();
    let first = fleet.admit(&lost, key.as_str()).await;
    let ring = capture.only(RING_FAILED);
    drop(capture);
    assert!(!first.replayed, "the first call inserts the row");
    assert_eq!(
        ring.field("fleet_id"),
        Some(fleet.fleet.as_str()),
        "the first notification was attempted, and lost"
    );

    let (bus, server) = afd_bus::test_util::detached().expect("an in-process bus needs no broker");
    let mut readers = wake_readers(&bus).await;
    let admissions = Admissions::for_tests(fleet.database.clone(), bus);
    let resent = fleet.admit(&admissions, key.as_str()).await;
    assert!(resent.replayed, "the resend replays the key");
    assert_eq!(
        resent.stored.id, first.stored.id,
        "the first call's id stands"
    );
    assert_eq!(
        rung(&server, &mut readers).await,
        [fleet.fleet.as_str()],
        "the retry publishes again"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1")
        .bind(&fleet.fleet)
        .fetch_one(&mut *fleet.database.acquire().await.expect("a pooled connection"))
        .await
        .expect("counting the fleet's admissions");
    assert_eq!(rows, 1, "the retry adds no queue state");
    fleet.cleanup().await;
}
