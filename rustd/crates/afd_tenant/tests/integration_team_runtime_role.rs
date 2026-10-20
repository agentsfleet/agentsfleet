//! Member removal served as the daemon's role.
//!
//! Every other team case connects as the lane's owner, which bypasses grants.
//! So when `identity.memberships` went immutable (no UPDATE grant), the
//! removal's `FOR UPDATE` stayed green in the suite while Postgres refused it
//! on dev. These cases run the team store over a pool holding only
//! `api_runtime`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::error_code;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_tenant::team::{Removal, Team};

use crate::access_lane::{Fixture, id};

/// A login role holding `api_runtime` and nothing else. `api_runtime` is
/// `NOLOGIN`, so a role of our own is the only way in.
const PROBE: &str = "tenant_runtime_probe";

/// Creates the probe role through the owner's pool; setup, not the subject.
/// The duplicate arms cover two suites racing on the shared lane.
async fn grant_probe(fixture: &Fixture) {
    let mut connection = fixture.database.acquire().await.expect("an owner connection");
    for statement in [
        "DO $$ BEGIN \
           EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', 'tenant_runtime_probe', 'tenant_runtime_probe'); \
         EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$",
        "GRANT api_runtime TO tenant_runtime_probe",
    ] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the lane's owner may create and grant to a probe role");
    }
}

/// A pool on the fixture's database that authenticates as [`PROBE`].
async fn runtime(fixture: &Fixture) -> Db {
    grant_probe(fixture).await;
    let mut url = url::Url::parse(&fixture.lane.url()).expect("the lane's URL is a URL");
    url.set_username(PROBE).expect("the URL takes a user");
    url.set_password(Some(PROBE)).expect("the URL takes a password");
    TestDatabase::shared()
        .open(
            DbRole::Api,
            &[
                (DbRole::Api.url_knob(), url.as_str()),
                ("DATABASE_POOL_SIZE", "2"),
                ("DATABASE_MIN_POOL_SIZE", "1"),
            ],
        )
        .await
}

/// John removes Bob, then cannot remove himself as the last owner, both as
/// the daemon's role.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_owner_removes_a_member_and_keeps_the_last_owner_as_api_runtime() {
    let fixture = Fixture::create().await;
    let database = runtime(&fixture).await;
    let team = Team::new(database.clone(), Entropy::new());
    let tenant = id(&fixture.john.tenant);

    let removed = team
        .remove(&tenant, &fixture.bob.user_id)
        .await
        .expect("api_runtime removes a member");
    assert_eq!(removed, Removal::Removed);

    let last = team
        .remove(&tenant, &fixture.john.user_id)
        .await
        .expect_err("the last owner stays");
    assert_eq!(last.code(), error_code::MEMBER_LAST_OWNER);

    database.close().await;
    fixture.cleanup().await;
}
