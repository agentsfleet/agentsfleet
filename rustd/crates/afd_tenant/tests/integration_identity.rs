//! `user_of` against the migrated schema — the read `GET /v1/users/me` renders.
//!
//! What only a real Postgres can prove here is the JOIN and the NULL. A stub
//! answering a `Profile` would be asserting the struct's field names; the
//! questions worth asking are whether the tenant name comes back with the user
//! in one round trip, whether a `display_name` that was never set arrives as
//! absent rather than as an empty string, and whether a subject with no row is
//! refused rather than provisioned. Each of those is the schema's answer.
//!
//! One walk over one fixture, not four fixtures: the reads are independent and
//! the seed is the expensive part.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::cli_credential::CliCredentials;

/// The subject the named person answers to.
const SUBJECT: &str = "user_identity_named";

/// The subject of a person who never gave a display name.
const SUBJECT_ANONYMOUS: &str = "user_identity_unnamed";

/// A subject no `core.users` row carries.
const SUBJECT_ABSENT: &str = "user_identity_absent";

const EMAIL: &str = "ada@identity.test";
const EMAIL_ANONYMOUS: &str = "unnamed@identity.test";
const DISPLAY_NAME: &str = "Ada Lovelace";
const TENANT_NAME: &str = "Ada's Workshop";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn profile_answers_the_joined_person_and_refuses_an_unknown_subject() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let identities = CliCredentials::new(fixture.database.clone(), Entropy::new());

    let named = identities
        .user_of(SUBJECT)
        .await
        .expect("a seeded subject resolves");
    assert_eq!(named.email, EMAIL);
    assert_eq!(
        named.display_name.as_deref(),
        Some(DISPLAY_NAME),
        "the display name comes back as stored"
    );
    assert_eq!(
        named.id.as_str(),
        fixture.user,
        "the user id is the core.users row, not the provider subject"
    );
    assert_eq!(
        named.tenant.as_str(),
        fixture.tenant,
        "the tenant is the joined user row's"
    );
    assert_eq!(
        named.tenant_name, TENANT_NAME,
        "the tenant NAME arrives with the user, in one round trip"
    );

    // A column that was never written must not arrive as an empty string: the
    // wire omits the key on `None`, and an empty string would publish a display
    // name nobody typed.
    let unnamed = identities
        .user_of(SUBJECT_ANONYMOUS)
        .await
        .expect("a subject with no display name still resolves");
    assert_eq!(unnamed.display_name, None);
    assert_eq!(unnamed.email, EMAIL_ANONYMOUS);
    assert_eq!(
        unnamed.tenant_name, TENANT_NAME,
        "both fixture people share one tenant, so both read its name"
    );

    // Refused rather than provisioned, and the refusal is the shared one the
    // credential family already raises — same code, same sentence.
    let refused = identities
        .user_of(SUBJECT_ABSENT)
        .await
        .expect_err("a subject with no row is refused");
    assert_eq!(
        refused.code(),
        afd_core::error_code::AUTH_FORBIDDEN,
        "an authenticating credential naming nobody is forbidden, not missing"
    );
    assert_eq!(
        refused.detail(),
        afd_tenant::error::DETAIL_UNKNOWN_SUBJECT,
        "the sentence is the family-neutral one, taken from its own constant"
    );

    // Nothing was written. The read path holds no INSERT, and this is the
    // assertion that keeps it that way after somebody edits the statement.
    assert_eq!(
        fixture.users_in_tenant().await,
        2,
        "a refused read must not provision the subject it could not find"
    );

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: afd_db::Db,
    tenant: String,
    user: String,
    anonymous_user: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            user: mint_id(),
            anonymous_user: mint_id(),
            lane,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, $2, 1, 1) \
             ) \
             INSERT INTO core.users \
               (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
             VALUES ($3::uuid, $1::uuid, $4, $5, $6, 1, 1), \
                    ($7::uuid, $1::uuid, $8, $9, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(TENANT_NAME)
        .bind(&self.user)
        .bind(SUBJECT)
        .bind(EMAIL)
        .bind(DISPLAY_NAME)
        .bind(&self.anonymous_user)
        .bind(SUBJECT_ANONYMOUS)
        .bind(EMAIL_ANONYMOUS)
        .execute(&mut *connection)
        .await
        .expect("the identity fixture seeds atomically");
    }

    /// How many people this fixture's own tenant holds.
    ///
    /// Scoped to the fixture's tenant, never a table-wide count: this crate's
    /// suites share one database and run in parallel, so a global count is the
    /// assertion that broke a sibling crate.
    async fn users_in_tenant(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM core.users WHERE tenant_id = $1::uuid")
                .bind(&self.tenant)
                .fetch_one(&mut *connection)
                .await
                .expect("the fixture's own row count reads");
        count
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the identity fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
