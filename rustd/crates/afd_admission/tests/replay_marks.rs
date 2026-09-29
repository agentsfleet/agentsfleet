//! A resend of an admitted key marks its fleet again.
//!
//! The first call's readiness mark is best-effort: a mark that does not land
//! is logged and the producer is answered anyway. The producer's resend under
//! the same key is the one caller left to notice, so a replay whose entry is
//! receipted marks the fleet again rather than leaving the entry stranded
//! until the reclaim sweep reaches it.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "../../afd_dragonfly/tests/support/subscriber.rs"]
mod subscriber;

#[path = "../../afd_dragonfly/tests/support/fake_redis.rs"]
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
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, DbRole};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_wire::event::EventType;

use self::fake_redis::{FakeRedis, Reply, install_subscriber};

/// The instant the seeded rows carry.
const SEED_MS: i64 = 1_760_000_000_000;

/// The entry id the fake's `XADD` hands back.
const FAKE_RECEIPT: &str = "1790000000000-0";

/// A hash write the index refuses.
const HSET_REFUSED: &str = "-ERR the index refused the write\r\n";

/// A hash write the index takes: one new field.
const HSET_TAKEN: &str = ":1\r\n";

/// The write a readiness mark is.
const CMD_HSET: &str = "HSET";

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
        for (statement, binds) in [
            (
                "INSERT INTO core.tenants (id, name, created_at, updated_at) \
                 VALUES ($1::uuid, $1::text, $3, $3)",
                [&seeded.tenant, &seeded.tenant],
            ),
            (
                "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
                 VALUES ($1::uuid, $2::uuid, $1::text, 'replay-fixture', $3)",
                [&seeded.workspace, &seeded.tenant],
            ),
        ] {
            seeded.execute(statement, binds).await;
        }
        sqlx::query(
            "INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, \
             config_json, status, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'replay-fixture-fleet', '# fixture', \
                     '{}'::jsonb, 'stopped', $4, $4)",
        )
        .bind(&seeded.fleet)
        .bind(&seeded.workspace)
        .bind(&seeded.tenant)
        .bind(SEED_MS)
        .execute(
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

    async fn execute(&self, statement: &'static str, [first, second]: [&String; 2]) {
        sqlx::query(statement)
            .bind(first)
            .bind(second)
            .bind(SEED_MS)
            .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("a seed row must insert");
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
            ("DELETE FROM core.fleets WHERE id = $1::uuid", &self.fleet),
            (
                "DELETE FROM core.workspaces WHERE id = $1::uuid",
                &self.workspace,
            ),
            ("DELETE FROM core.tenants WHERE id = $1::uuid", &self.tenant),
        ] {
            let _removed = sqlx::query(statement)
                .bind(id)
                .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
                .await;
        }
        self.handle.cleanup().await;
    }
}

/// How many readiness marks the fake was sent.
fn marks(server: &FakeRedis) -> usize {
    server
        .seen()
        .iter()
        .filter(|command| command.starts_with(CMD_HSET))
        .count()
}

/// The first call's mark is refused; the resend replays the key and marks the
/// fleet, and this time the mark lands.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_resend_marks_the_fleet_its_first_call_could_not() {
    install_subscriber();
    let fleet = Seeded::seed().await;
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("XADD", Reply::Bulk(FAKE_RECEIPT)),
        (CMD_HSET, Reply::Raw(HSET_REFUSED)),
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
    assert_eq!(marks(&server), 1, "the first call tried to mark");

    server.set_reply(CMD_HSET, Reply::Raw(HSET_TAKEN));
    let resent = fleet.admit(&admissions, &key).await;
    assert!(resent.replayed, "the resend replays the key");
    assert_eq!(
        resent.stored.id, first.stored.id,
        "the first call's id stands"
    );
    assert_eq!(marks(&server), 2, "the replay marked the fleet again");
    fleet.cleanup().await;
}
