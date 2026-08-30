//! Opening a personal account over a live Postgres.
//!
//! `identity_signup_route.rs` proves everything the route refuses before it
//! reaches a store. This proves what happens after: the five rows land together
//! or not at all, a replay answers as the first delivery did, and a wallet that
//! went missing comes back.
//!
//! # Every case mints its own subject
//!
//! `core.users.oidc_subject` carries a GLOBAL unique index, so two cases in one
//! file sharing a subject would race: whichever lost would trip the index and
//! the other would then fail for an unrelated reason. One subject per case,
//! minted, never a constant.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::signup::{NewAccount, STARTER_CREDIT_NANOS, Signups, personal_tenant_name};

/// The instant every row is stamped with.
const SEED_MS: i64 = 1_760_000_000_000;

/// The address every case opens an account under.
const EMAIL: &str = "ada@example.test";

fn now() -> UnixMillis {
    UnixMillis::from_millis(SEED_MS)
}

/// A subject nothing else in the lane addresses.
fn subject() -> String {
    format!("user_{}", mint_id().replace('-', ""))
}

/// The provisioning surface, over the lane's Postgres.
async fn lane() -> (TestDatabase, Signups) {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let signups = Signups::new(database, Entropy::new());
    (lane, signups)
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_signup_opens_every_row_an_account_needs() {
    let (lane, signups) = lane().await;
    let subject = subject();
    let tenant_name = personal_tenant_name(EMAIL).expect("the fixture address has a local part");

    let opened = signups
        .bootstrap(
            NewAccount {
                oidc_subject: &subject,
                email: EMAIL,
                display_name: Some("Ada Lovelace"),
            },
            tenant_name,
            now(),
        )
        .await
        .expect("the lane's Postgres must answer");

    assert!(opened.created, "a first delivery opens the account");
    assert!(
        !opened.workspace_name.is_empty(),
        "a workspace nobody named still gets one, so it can be talked about"
    );

    // Every row, read back independently. Asserting on the returned struct
    // alone would pass for a bootstrap that composed the right answer and
    // committed none of it.
    let mut connection = lane.open(DbRole::Api, &[]).await.acquire().await.expect("a connection");
    let user: (String, String) = sqlx::query_as(
        "SELECT tenant_id::text, email FROM core.users WHERE oidc_subject = $1",
    )
    .bind(&subject)
    .fetch_one(&mut *connection)
    .await
    .expect("the user row lands");
    assert_eq!(user.0, opened.tenant_id);
    assert_eq!(user.1, EMAIL);

    let role: String = sqlx::query_scalar(
        "SELECT role FROM core.memberships WHERE user_id = $1::uuid AND tenant_id = $2::uuid",
    )
    .bind(&opened.user_id)
    .bind(&opened.tenant_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the membership row lands");
    assert_eq!(role, "owner", "a personal account's one member owns it");

    let workspace: (String, String) = sqlx::query_as(
        "SELECT tenant_id::text, name FROM core.workspaces WHERE id = $1::uuid",
    )
    .bind(&opened.workspace_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the workspace row lands");
    assert_eq!(workspace.0, opened.tenant_id);
    assert_eq!(workspace.1, opened.workspace_name);

    let balance: i64 = sqlx::query_scalar(
        "SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id = $1::uuid",
    )
    .bind(&opened.tenant_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the wallet row lands");
    assert_eq!(
        balance, STARTER_CREDIT_NANOS,
        "a tenant with no wallet answers 500 on every billing read, so the \
         grant is part of the bootstrap rather than a later step"
    );

    let tenant: String = sqlx::query_scalar("SELECT name FROM core.tenants WHERE id = $1::uuid")
        .bind(&opened.tenant_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the tenant row lands");
    assert_eq!(tenant, "ada", "the tenant is named for the address's local part");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_replayed_delivery_answers_as_the_first_one_did() {
    // The commonest thing that happens to this surface. A provider retries, and
    // a retry must not open a second account or fail.
    let (_lane, signups) = lane().await;
    let subject = subject();
    let account = NewAccount {
        oidc_subject: &subject,
        email: EMAIL,
        display_name: None,
    };
    let tenant_name = personal_tenant_name(EMAIL).expect("the fixture address has a local part");

    let first = signups
        .bootstrap(account, tenant_name, now())
        .await
        .expect("the lane's Postgres must answer");
    let second = signups
        .bootstrap(account, tenant_name, now())
        .await
        .expect("a replay is a success, never an error");

    assert!(first.created, "the first delivery opened the account");
    assert!(!second.created, "the second one did not, and says so");
    assert_eq!(second.user_id, first.user_id);
    assert_eq!(second.tenant_id, first.tenant_id);
    assert_eq!(
        second.workspace_id, first.workspace_id,
        "a replay resolves the SAME workspace, never a second one"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_replay_restores_a_wallet_that_went_missing() {
    // Only the create path writes the wallet, so a tenant that lost the row —
    // a bootstrap from before the grant existed, a restore — would 500 on every
    // billing read with no path back. The replay is the converging write.
    let (lane, signups) = lane().await;
    let subject = subject();
    let account = NewAccount {
        oidc_subject: &subject,
        email: EMAIL,
        display_name: None,
    };
    let tenant_name = personal_tenant_name(EMAIL).expect("the fixture address has a local part");

    let opened = signups
        .bootstrap(account, tenant_name, now())
        .await
        .expect("the lane's Postgres must answer");

    let mut connection = lane.open(DbRole::Api, &[]).await.acquire().await.expect("a connection");
    sqlx::query("DELETE FROM billing.tenant_wallet WHERE tenant_id = $1::uuid")
        .bind(&opened.tenant_id)
        .execute(&mut *connection)
        .await
        .expect("the fixture removes the wallet it is about to have healed");

    signups
        .bootstrap(account, tenant_name, now())
        .await
        .expect("a replay is a success");

    let balance: i64 = sqlx::query_scalar(
        "SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id = $1::uuid",
    )
    .bind(&opened.tenant_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the replay put the wallet back");
    assert_eq!(balance, STARTER_CREDIT_NANOS);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_replay_never_resets_a_balance_already_spent() {
    // The other half of the heal, and the one that would hurt: `ON CONFLICT DO
    // NOTHING` must leave a spent-down wallet exactly as it found it. A replay
    // that topped an account back up would be free credit for anyone who can
    // make a provider retry.
    let (lane, signups) = lane().await;
    let subject = subject();
    let account = NewAccount {
        oidc_subject: &subject,
        email: EMAIL,
        display_name: None,
    };
    let tenant_name = personal_tenant_name(EMAIL).expect("the fixture address has a local part");

    let opened = signups
        .bootstrap(account, tenant_name, now())
        .await
        .expect("the lane's Postgres must answer");

    let spent = STARTER_CREDIT_NANOS / 4;
    let mut connection = lane.open(DbRole::Api, &[]).await.acquire().await.expect("a connection");
    sqlx::query("UPDATE billing.tenant_wallet SET balance_nanos = $2 WHERE tenant_id = $1::uuid")
        .bind(&opened.tenant_id)
        .bind(spent)
        .execute(&mut *connection)
        .await
        .expect("the fixture spends the grant down");

    signups
        .bootstrap(account, tenant_name, now())
        .await
        .expect("a replay is a success");

    let balance: i64 = sqlx::query_scalar(
        "SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id = $1::uuid",
    )
    .bind(&opened.tenant_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the wallet is still there");
    assert_eq!(
        balance, spent,
        "a replay heals an ABSENT wallet and never refills a present one"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_concurrent_deliveries_open_exactly_one_account() {
    // The case the pre-read cannot cover, because it runs before the
    // transaction: both deliveries pass it, one commits, and the other must
    // resolve to the winner's rows rather than raising or opening a second
    // account. The unique index is the arbiter.
    let (lane, signups) = lane().await;
    let subject = subject();
    let tenant_name = personal_tenant_name(EMAIL).expect("the fixture address has a local part");

    let one = signups.clone();
    let two = signups.clone();
    let (left, right) = {
        let subject = subject.clone();
        let other = subject.clone();
        tokio::join!(
            async move {
                one.bootstrap(
                    NewAccount {
                        oidc_subject: &subject,
                        email: EMAIL,
                        display_name: None,
                    },
                    tenant_name,
                    now(),
                )
                .await
            },
            async move {
                two.bootstrap(
                    NewAccount {
                        oidc_subject: &other,
                        email: EMAIL,
                        display_name: None,
                    },
                    tenant_name,
                    now(),
                )
                .await
            }
        )
    };

    let left = left.expect("neither racer raises");
    let right = right.expect("neither racer raises");
    assert_eq!(
        left.user_id, right.user_id,
        "both deliveries resolve to ONE account"
    );
    assert_eq!(left.tenant_id, right.tenant_id);
    assert!(
        left.created ^ right.created,
        "exactly one of them opened it; the other reports a replay"
    );

    let mut connection = lane.open(DbRole::Api, &[]).await.acquire().await.expect("a connection");
    let users: i64 =
        sqlx::query_scalar("SELECT count(*) FROM core.users WHERE oidc_subject = $1")
            .bind(&subject)
            .fetch_one(&mut *connection)
            .await
            .expect("counting the subject's users");
    assert_eq!(users, 1, "one subject, one person");
}
