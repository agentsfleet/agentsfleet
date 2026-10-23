//! Retrying an admitted key republishes its hint without adding queue state.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "../../afd_dragonfly/tests/support/subscriber.rs"]
mod subscriber;

#[path = "../../afd_dragonfly/tests/support/fake_redis/mod.rs"]
#[allow(
    unused_imports,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::format_push_string,
    reason = "test support shared with `afd_dragonfly`, which uses the half this suite does not"
)]
mod fake_redis;

use std::time::Duration;

use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_db::{Db, DbRole};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_wire::event::EventType;
use jiff::Timestamp;

use self::fake_redis::{FakeRedis, Reply, install_subscriber};

/// The instant the seeded rows carry.
const SEEDED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The status that keeps every lease path off the fixture fleet.
const FLEET_STOPPED: &str = "stopped";

const CMD_SPUBLISH: &str = "SPUBLISH";
const PUBLISH_REFUSED: &str = "-ERR notification refused\r\n";
const PUBLISH_TAKEN: &str = ":0\r\n";

/// A stopped fleet and the rows above it, removed by [`Seeded::cleanup`].
struct Seeded {
    handle: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    fleet: String,
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
            let _removed = sqlx::query(statement)
                .bind(id)
                .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
                .await;
        }
        self.handle.cleanup().await;
    }
}

fn rings(server: &FakeRedis) -> usize {
    sent(server, CMD_SPUBLISH)
}

fn sent(server: &FakeRedis, verb: &str) -> usize {
    server
        .seen()
        .iter()
        .filter(|command| command.starts_with(verb))
        .count()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retry_republishes_a_lost_notification_for_the_same_admission() {
    install_subscriber();
    let fleet = Seeded::seed().await;
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        (CMD_SPUBLISH, Reply::Raw(PUBLISH_REFUSED)),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2));
    let queue = Dragonfly::connect(&config)
        .await
        .expect("a fake that answers PONG must be accepted");
    let admissions = Admissions::for_tests(fleet.database.clone(), queue);
    let key = mint_id();

    let first = fleet.admit(&admissions, &key).await;
    assert!(!first.replayed, "the first call inserts the row");
    assert_eq!(rings(&server), 1, "the first notification was attempted");
    server.set_reply(CMD_SPUBLISH, Reply::Raw(PUBLISH_TAKEN));
    let resent = fleet.admit(&admissions, &key).await;
    assert!(resent.replayed, "the resend replays the key");
    assert_eq!(
        resent.stored.id, first.stored.id,
        "the first call's id stands"
    );
    assert_eq!(rings(&server), 2, "the retry publishes again");
    assert!(
        !server
            .seen()
            .iter()
            .any(|s| s.starts_with("XADD") || s.starts_with("HSET"))
    );
    fleet.cleanup().await;
}
