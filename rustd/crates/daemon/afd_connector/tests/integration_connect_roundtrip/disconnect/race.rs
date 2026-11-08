//! A Disconnect racing a connect, held at a row lock until both have queued.
//!
//! Both writers touch the sealed handle's row, so whichever order its lock
//! releases them in, the handle and the routing rows end agreeing. A first
//! connect has no handle row yet and is deliberately unordered (PR #778).

use std::time::Duration;

use afd_connector::grant::Forgotten;

use super::*;

/// How long a case waits for a writer to queue behind a held row lock.
const QUEUE_DEADLINE: Duration = Duration::from_secs(10);

/// How often it looks.
const QUEUE_POLL: Duration = Duration::from_millis(20);

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reconnect_racing_a_disconnect_leaves_both_rows_or_neither() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = NOW;
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

    // Hold the handle's row so both writers queue on it, the reconnect first,
    // then release them together.
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
    now: Timestamp,
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
