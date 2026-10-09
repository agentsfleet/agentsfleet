//! Disconnect against the grant a connect landed: both rows go, or neither.
//!
//! The handle in the vault and the routing rows that send a provider account's
//! events to a workspace describe one installation. A reader that finds one
//! without the other is wrong in a way nothing reports, so every case here ends
//! by asking the store whether the two still agree.

use std::time::Duration;

use afd_connector::grant::{Forgotten, Grant, Install, InstallClaim};
use serde_json::{Map, Value};

use super::*;

/// The provider these cases connect: one with inbound routing rows.
const PROVIDER: Provider = Provider::Slack;

/// The account the first connect routes, before [`account`] scopes it.
const ACCOUNT_A: &str = "T0FIXTUREA";

/// The account a reconnect routes in its place.
const ACCOUNT_B: &str = "T0FIXTUREB";

/// How long a case waits for a writer to queue behind a held row lock.
const QUEUE_DEADLINE: Duration = Duration::from_secs(10);

/// How often it looks.
const QUEUE_POLL: Duration = Duration::from_millis(20);

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_disconnect_whose_vault_delete_is_refused_keeps_its_routing_rows() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = UnixMillis::from_millis(NOW_MS);
    round
        .grants()
        .land(
            &round.workspace,
            PROVIDER,
            &grant(&account(&round, ACCOUNT_A)),
            now,
        )
        .await
        .expect("the first connect lands");
    // A model entry naming the grant's key is what the vault refuses a delete
    // over; it makes the second write of a Disconnect fail on demand.
    let entry = reference_grant(&round).await;

    round
        .grants()
        .forget(&round.workspace, PROVIDER)
        .await
        .expect_err("the vault refuses a credential the registry still names");

    assert_eq!(
        routing_rows(&round).await,
        1,
        "a refused Disconnect removed the routing row and left its handle"
    );
    assert!(handle_held(&round).await, "and the handle is still there");

    drop_entry(&round, &entry).await;
    round.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reconnect_racing_a_disconnect_leaves_both_rows_or_neither() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = UnixMillis::from_millis(NOW_MS);
    round
        .grants()
        .land(
            &round.workspace,
            PROVIDER,
            &grant(&account(&round, ACCOUNT_A)),
            now,
        )
        .await
        .expect("the first connect lands");

    // Hold the handle's row so both writers queue behind it, the reconnect
    // first, then release them together.
    let mut holder = round.database.acquire().await.expect("a pooled connection");
    let mut held = sqlx::Acquire::begin(&mut *holder)
        .await
        .expect("the holder's transaction opens");
    sqlx::query(
        "SELECT 1 FROM vault.secrets \
         WHERE workspace_id = $1::uuid AND key_name = $2 FOR UPDATE",
    )
    .bind(round.workspace.as_str())
    .bind(PROVIDER.grant_key())
    .fetch_one(&mut *held)
    .await
    .expect("the first connect's handle is there to hold");

    let reconnect = {
        let grants = round.grants();
        let workspace = round.workspace.clone();
        let replacement = grant(&account(&round, ACCOUNT_B));
        tokio::spawn(async move { grants.land(&workspace, PROVIDER, &replacement, now).await })
    };
    queued_on_a_lock(&round, 1).await;
    let disconnect = {
        let grants = round.grants();
        let workspace = round.workspace.clone();
        tokio::spawn(async move { grants.forget(&workspace, PROVIDER).await })
    };
    queued_on_a_lock(&round, 2).await;
    held.commit().await.expect("the holder releases");

    // Either may win the row. A reconnect that loses finds no handle to
    // replace and rolls back; one that wins is then disconnected.
    let _either = reconnect.await.expect("the reconnect task completes");
    let forgotten = disconnect
        .await
        .expect("the disconnect task completes")
        .expect("the disconnect commits");
    assert!(matches!(
        forgotten,
        Forgotten::Disconnected | Forgotten::AlreadyAbsent
    ));
    let routed = routing_rows(&round).await;
    let held_after = handle_held(&round).await;
    assert_eq!(
        routed > 0,
        held_after,
        "routing rows ({routed}) and the handle (held: {held_after}) disagree"
    );

    round.cleanup().await;
}

/// `label` scoped to this round's workspace. Routing rows are unique by
/// provider and account across every workspace, so two cases sharing an
/// account would repoint each other's rows when the lane runs them together.
fn account(round: &Round, label: &str) -> String {
    format!("{label}-{}", round.workspace.as_str())
}

/// A Slack grant routing `account` to whichever workspace lands it.
fn grant(account: &str) -> Grant {
    let mut handle = Map::new();
    handle.insert("integration".to_owned(), Value::from("slack"));
    handle.insert(
        "bot_token".to_owned(),
        Value::from(format!("xoxb-{account}")),
    );
    Grant {
        handle,
        install: Some(Install {
            external_account_id: account.to_owned(),
            installed_by: SUBJECT.to_owned(),
            scopes: Vec::new(),
            claim: InstallClaim::Repoint,
        }),
    }
}

/// How many routing rows send this provider's events to the round's workspace.
async fn routing_rows(round: &Round) -> i64 {
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query_scalar(
        "SELECT count(*) FROM core.connector_installs \
         WHERE provider = $1 AND workspace_id = $2::uuid",
    )
    .bind(PROVIDER.id())
    .bind(round.workspace.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the routing read runs")
}

/// Whether the round's workspace still holds this provider's handle.
async fn handle_held(round: &Round) -> bool {
    let name = SecretName::parse(PROVIDER.grant_key()).expect("the grant key parses");
    round
        .vault
        .load(&round.workspace, &name)
        .await
        .expect("the vault read runs")
        .is_some()
}

/// Names the grant's key from the tenant's model registry.
async fn reference_grant(round: &Round) -> String {
    let id = mint_id();
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.tenant_model_entries \
           (id, tenant_id, model_id, secret_ref, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, 'fixture-model', $3, $4, $4)",
    )
    .bind(&id)
    .bind(&round.tenant)
    .bind(PROVIDER.grant_key())
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the model entry inserts");
    id
}

/// Removes the entry [`reference_grant`] wrote.
async fn drop_entry(round: &Round, id: &str) {
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query("DELETE FROM core.tenant_model_entries WHERE id = $1::uuid")
        .bind(id)
        .execute(&mut *connection)
        .await
        .expect("the model entry deletes");
}

/// Waits until `waiters` of the connection's writers are queued on a lock.
///
/// Either table counts: a writer that touches routing before the vault queues
/// on a routing row instead, and that ordering is what a case must observe.
async fn queued_on_a_lock(round: &Round, waiters: i64) {
    let deadline = tokio::time::Instant::now() + QUEUE_DEADLINE;
    loop {
        let mut connection = round.database.acquire().await.expect("a pooled connection");
        let queued: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock' \
               AND (query LIKE '%vault.secrets%' OR query LIKE '%connector_installs%')",
        )
        .fetch_one(&mut *connection)
        .await
        .expect("the activity read runs");
        if queued >= waiters {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "only {queued} of {waiters} writers queued"
        );
        tokio::time::sleep(QUEUE_POLL).await;
    }
}
