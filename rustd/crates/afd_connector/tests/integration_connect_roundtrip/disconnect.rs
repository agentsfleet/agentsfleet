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

    let refused = round
        .grants()
        .forget(&round.workspace, PROVIDER)
        .await
        .expect_err("the vault refuses a credential the registry still names");

    assert_eq!(
        refused.code(),
        afd_core::error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES,
        "the refusal is the vault's conflict, not an operation failure: {refused}"
    );
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

    // Hold the handle's row so both writers queue, the reconnect first, then
    // release them together. The Disconnect queues behind the reconnect's
    // turn on the workspace row, not behind the handle.
    let (mut holder, holder_pid) = hold(
        &round,
        "SELECT 1 FROM vault.secrets \
         WHERE workspace_id = $1::uuid AND key_name = $2 FOR UPDATE",
        &[round.workspace.as_str(), PROVIDER.grant_key()],
    )
    .await;
    let reconnect = spawn_land(&round, &account(&round, ACCOUNT_B), now);
    queued_on_a_lock(&round, holder_pid, 1).await;
    let disconnect = spawn_forget(&round);
    queued_on_a_lock(&round, holder_pid, 2).await;
    holder.release().await;

    reconnect
        .await
        .expect("the reconnect task completes")
        .expect("the reconnect lands: it took the workspace's turn first");
    let forgotten = disconnect
        .await
        .expect("the disconnect task completes")
        .expect("the disconnect commits");
    assert_eq!(
        forgotten,
        Forgotten::Disconnected,
        "it removed the reconnect's grant"
    );
    assert_both_or_neither(&round).await;

    round.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_disconnect_racing_a_first_connect_waits_its_turn() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = UnixMillis::from_millis(NOW_MS);

    // No handle exists yet, so nothing in the vault can order these two. Both
    // must queue on the workspace row; a path that skipped it would not wait,
    // and could delete routing rows the connect commits mid-Disconnect.
    let (mut holder, holder_pid) = hold(
        &round,
        "SELECT 1 FROM core.workspaces WHERE id = $1::uuid FOR NO KEY UPDATE",
        &[round.workspace.as_str()],
    )
    .await;
    let connect = spawn_land(&round, &account(&round, ACCOUNT_A), now);
    queued_on_a_lock(&round, holder_pid, 1).await;
    let disconnect = spawn_forget(&round);
    queued_on_a_lock(&round, holder_pid, 2).await;
    holder.release().await;

    connect
        .await
        .expect("the connect task completes")
        .expect("the first connect lands");
    let forgotten = disconnect
        .await
        .expect("the disconnect task completes")
        .expect("the disconnect commits");
    assert_eq!(
        forgotten,
        Forgotten::Disconnected,
        "it removed what the connect landed"
    );
    assert_both_or_neither(&round).await;

    round.cleanup().await;
}

/// A transaction holding one row lock, so writers queue behind it.
///
/// On a connection detached from the pool: a case that fails while holding
/// drops it, Postgres rolls the hold back, and no pooled connection is ever
/// returned inside an open transaction.
struct Holder {
    connection: sqlx::PgConnection,
}

impl Holder {
    /// Commits, letting every queued writer go.
    async fn release(&mut self) {
        sqlx::query("COMMIT")
            .execute(&mut self.connection)
            .await
            .expect("the holder releases");
    }
}

/// Takes the lock `statement` names, bound to `binds` in order, and answers the
/// holding backend's pid.
async fn hold(round: &Round, statement: &'static str, binds: &[&str]) -> (Holder, i32) {
    let mut connection = round
        .database
        .acquire()
        .await
        .expect("a pooled connection")
        .detach();
    sqlx::query("BEGIN")
        .execute(&mut connection)
        .await
        .expect("the holder's transaction opens");
    binds
        .iter()
        .fold(sqlx::query(statement), |query, bind| query.bind(*bind))
        .fetch_one(&mut connection)
        .await
        .expect("the row is there to hold");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut connection)
        .await
        .expect("the holder names itself");
    (Holder { connection }, pid)
}

/// Lands `account`'s grant on its own task.
fn spawn_land(
    round: &Round,
    account: &str,
    now: UnixMillis,
) -> tokio::task::JoinHandle<afd_connector::Result<()>> {
    let grants = round.grants();
    let workspace = round.workspace.clone();
    let landed = grant(account);
    tokio::spawn(async move { grants.land(&workspace, PROVIDER, &landed, now).await })
}

/// Disconnects the round's workspace on its own task.
fn spawn_forget(round: &Round) -> tokio::task::JoinHandle<afd_connector::Result<Forgotten>> {
    let grants = round.grants();
    let workspace = round.workspace.clone();
    tokio::spawn(async move { grants.forget(&workspace, PROVIDER).await })
}

/// The handle and the routing rows agree: both present or both gone.
async fn assert_both_or_neither(round: &Round) {
    let routed = routing_rows(round).await;
    let held = handle_held(round).await;
    assert_eq!(
        routed > 0,
        held,
        "routing rows ({routed}) and the handle (held: {held}) disagree"
    );
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

/// Waits until `waiters` writers are queued behind `holder`, directly or behind
/// a writer the holder blocks.
///
/// Scoped to the holder's chain: the lane shares one database, and another
/// test's lock wait must not stand in for this case's writers.
async fn queued_on_a_lock(round: &Round, holder: i32, waiters: i64) {
    let deadline = tokio::time::Instant::now() + QUEUE_DEADLINE;
    loop {
        let mut connection = round.database.acquire().await.expect("a pooled connection");
        let queued: i64 = sqlx::query_scalar(
            "WITH first AS ( \
               SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) \
             ) \
             SELECT count(*) FROM pg_stat_activity a \
             WHERE $1 = ANY(pg_blocking_pids(a.pid)) \
                OR pg_blocking_pids(a.pid) && ARRAY(SELECT pid FROM first)",
        )
        .bind(holder)
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
