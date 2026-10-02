//! Two logins from one machine at once, against the live partial index that
//! holds a machine to one live credential. The index is the only arbiter, so
//! only a real Postgres can prove the mint's one retry.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_tenant::cli_credential::{CliCredentials, MachineName, MintRequest};

use crate::integration_identity::Fixture;

/// The terminal two logins race on, and what each mint records about itself.
const MACHINE: &str = "identity-race.local";
const DEPLOYMENT: &str = "https://api.identity.test";
const FROM_ADDRESS: &str = "127.0.0.1";
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// The racer's address and account name.
const RACER_EMAIL: &str = "racer@identity.test";
const RACER_TENANT_NAME: &str = "Ada's Racetrack";

/// How often, and how many times, the race polls for both logins waiting.
const POLL_EVERY: Duration = Duration::from_millis(10);
const POLL_ATTEMPTS: usize = 500;

/// Two logins from one machine at once both land and leave one live
/// credential. Ada's row is held locked, so both mints reach the insert before
/// either commits: the index refuses the second, and the mint retries it once,
/// revoking the first and taking its place.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_logins_from_one_machine_at_once_leave_one_live_credential() {
    let fixture = Fixture::create().await;
    fixture.seed_racer().await;
    let credentials = CliCredentials::new(fixture.database.clone(), Entropy::new());
    let (user, tenant) = (parsed(&fixture.user), parsed(&fixture.tenant));
    let request = MintRequest {
        user: &user,
        tenant: &tenant,
        machine: MachineName::parse(MACHINE).expect("a well-formed machine name"),
        deployment: DEPLOYMENT,
        from_address: FROM_ADDRESS,
    };

    let mut blocker = fixture.database.acquire().await.expect("an API connection");
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .expect("the holder names itself");
    let mut hold = sqlx::Connection::begin(&mut *blocker)
        .await
        .expect("the hold opens");
    sqlx::query("SELECT 1 FROM core.users WHERE id = $1::uuid FOR UPDATE")
        .bind(&fixture.user)
        .execute(&mut *hold)
        .await
        .expect("Ada's row is held");
    let release = async {
        assert!(
            fixture.both_wait_behind(blocker_pid).await,
            "both reach the insert"
        );
        hold.commit().await.expect("the hold releases");
    };
    let (first, second, ()) = tokio::join!(
        credentials.mint(&request, NOW),
        credentials.mint(&request, NOW),
        release,
    );
    let minted = [
        first.expect("one login lands"),
        second.expect("the other lands"),
    ];

    let live = fixture.the_one_live_credential_on(MACHINE).await;
    assert!(minted.iter().any(|login| login.id.as_str() == live));
    fixture.cleanup().await;
}

fn parsed(stored: &str) -> Uuid7 {
    Uuid7::parse(stored).expect("the fixture identifier is UUIDv7")
}

impl Fixture {
    /// Ada alone, under a subject of this fixture's own: the race runs beside
    /// the profile walk, and a subject is unique across the lane.
    async fn seed_racer(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, $2, 1, 1) \
             ) \
             INSERT INTO core.users \
               (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
             VALUES ($3::uuid, $1::uuid, $4, $5, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(RACER_TENANT_NAME)
        .bind(&self.user)
        .bind(format!("user_identity_race_{}", self.user))
        .bind(RACER_EMAIL)
        .execute(&mut *connection)
        .await
        .expect("the racer seeds atomically");
    }

    /// Whether both mints came to wait at the insert: one behind the session
    /// `holder`, which holds Ada's row, and one behind that first mint, whose
    /// uncommitted row the index must wait on. Polled for up to five seconds.
    async fn both_wait_behind(&self, holder: i32) -> bool {
        for _attempt in 0..POLL_ATTEMPTS {
            let mut connection = self.database.acquire().await.expect("an API connection");
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity AS waiting \
                 WHERE $1 = ANY(pg_blocking_pids(waiting.pid)) \
                    OR EXISTS (SELECT 1 FROM pg_stat_activity AS first \
                               WHERE $1 = ANY(pg_blocking_pids(first.pid)) \
                                 AND first.pid = ANY(pg_blocking_pids(waiting.pid)))",
            )
            .bind(holder)
            .fetch_one(&mut *connection)
            .await
            .expect("the activity view reads");
            if waiting == 2 {
                return true;
            }
            drop(connection);
            tokio::time::sleep(POLL_EVERY).await;
        }
        false
    }

    /// The one live credential Ada holds for `machine`, after asserting it
    /// sits beside exactly one revoked row: one row per login, one live.
    async fn the_one_live_credential_on(&self, machine: &str) -> String {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let rows: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT id::text, revoked_at FROM core.cli_credentials \
             WHERE user_id = $1::uuid AND machine_name = $2",
        )
        .bind(&self.user)
        .bind(machine)
        .fetch_all(&mut *connection)
        .await
        .expect("the credentials read");
        let mut live = rows.iter().filter(|(_, revoked_at)| revoked_at.is_none());
        let (id, _) = live.next().expect("one credential is live");
        assert!(
            live.next().is_none(),
            "one live credential per machine: {rows:?}"
        );
        assert_eq!(rows.len(), 2, "one row per login: {rows:?}");
        id.clone()
    }
}
