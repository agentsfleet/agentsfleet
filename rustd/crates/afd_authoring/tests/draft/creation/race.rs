//! Kill and activation contend for one fleet row and linearise: whichever
//! commits first decides, and the other observes it.
//!
//! Each order is forced, never hoped for. A transaction on its own connection
//! holds the one row the chosen side reaches next, the test waits until
//! Postgres reports that side blocked on it, then lets the other side run.
//! Every wait is bounded, so a lock-order deadlock fails the proof instead of
//! hanging it.

use super::super::{fixture::now, mint};
use super::fixture::{Case, NO_ADMISSIONS};
use afd_authoring::{
    Result, SavedFleet,
    types::{DraftState, SaveFleet, Start, StartOutcome, StartState},
};
use afd_core::{error_code, id::Uuid7};
use sqlx::{Connection as _, PgConnection};
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;

/// Long enough for any healthy interleaving; a deadlock outlasts it.
const BOUND: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);
/// The wallet row activation locks after its unlocked readiness reads.
const HOLD_WALLET: &str =
    "SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1::uuid FOR UPDATE";
/// The linked draft row activation closes while it holds the fleet row.
const HOLD_DRAFT: &str = "SELECT id FROM authoring.drafts WHERE id=$1::uuid FOR NO KEY UPDATE";
/// One backend waiting directly or transitively on a holder while running a
/// statement that starts with the given text. Fixture table locks may intervene.
const WAITER: &str = "WITH RECURSIVE blocked(pid) AS ( \
SELECT unnest($1::int[]) UNION \
SELECT a.pid FROM pg_stat_activity a JOIN blocked b ON b.pid=ANY(pg_blocking_pids(a.pid))) \
SELECT pid FROM pg_stat_activity WHERE pid IN (SELECT pid FROM blocked) \
AND NOT (pid=ANY($1::int[])) AND starts_with(query,$2) ORDER BY pid LIMIT 1";
/// Where activation waits when the wallet row is held: past every unlocked check.
const AT_WALLET: &str = "SELECT balance_nanos FROM billing.wallets";
/// Where activation waits when its draft row is held: holding the fleet row it
/// has just activated, before its transaction commits.
const AT_CLOSE: &str = "UPDATE authoring.drafts\n   SET status = $2, closed_at";
/// Kill's fleet row lock, the only read that could wait on an activation.
const AT_LOCK: &str = "SELECT";

/// One row lock held open on a connection of its own.
struct Hold {
    connection: PgConnection,
    pid: i32,
}

impl Hold {
    async fn take(url: &str, statement: &'static str, row: &str) -> Self {
        let mut connection = PgConnection::connect(url).await.expect("holder connects");
        let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut connection)
            .await
            .expect("holder backend");
        for step in ["BEGIN", statement] {
            sqlx::query(step)
                .bind(row)
                .execute(&mut connection)
                .await
                .expect("hold the row");
        }
        Self { connection, pid }
    }

    async fn release(mut self) {
        sqlx::query("COMMIT")
            .execute(&mut self.connection)
            .await
            .expect("release the row");
    }
}

/// Waits until a backend blocked by one of `holders` is running `statement`,
/// and answers its process ID. Other sessions on the shared lane can queue
/// behind the same holders (a trigger install waits on any open write to its
/// table), so the statement, not the count, identifies the contender.
///
/// The census reads as the lane owner: `pg_stat_activity` hides another
/// login's statement text, and the holders and the table lock run as the
/// owner while the contenders run as the API login.
async fn blocked(c: &Case, holders: &[i32], statement: &str) -> i32 {
    let mut census = PgConnection::connect(&c.base.owner_url())
        .await
        .expect("census connects");
    bounded(statement, async {
        loop {
            let waiting: Option<i32> = sqlx::query_scalar(WAITER)
                .bind(holders)
                .bind(statement)
                .fetch_optional(&mut census)
                .await
                .expect("lock census");
            if let Some(pid) = waiting {
                return pid;
            }
            tokio::time::sleep(POLL).await;
        }
    })
    .await
}

async fn bounded<T>(what: &str, work: impl Future<Output = T>) -> T {
    tokio::time::timeout(BOUND, work)
        .await
        .unwrap_or_else(|_elapsed| unreachable_within(what))
}

#[expect(
    clippy::panic,
    reason = "a wait past the bound is the deadlock this proves absent"
)]
fn unreachable_within(what: &str) -> ! {
    panic!("no wait at `{what}` resolved within {BOUND:?}: a deadlock or a lost wake-up")
}

/// A funded fleet still `installing` from an unfunded Save, and what its retry needs.
struct Installing {
    fleet: Uuid7,
    key: Uuid7,
    request: SaveFleet,
    tag: String,
    draft: Uuid7,
}

impl Installing {
    async fn save(c: &Case, name: &str) -> Self {
        let start = Start::RunNow {
            message: "Run after setup".to_owned(),
        };
        let (draft, request) = c.request(name, start).await;
        let key = mint();
        let saved = c.save(&key, request.clone()).await;
        assert_eq!(saved.status, "installing");
        c.fund().await;
        Self {
            fleet: saved.id,
            key,
            request,
            tag: saved.etag,
            draft,
        }
    }
}

/// The same-key Save retry that activates the funded fleet, on its own task.
fn activate(c: &Arc<Case>, key: Uuid7, request: SaveFleet) -> JoinHandle<Result<SavedFleet>> {
    let c = Arc::clone(c);
    tokio::spawn(async move {
        c.creations
            .save(c.base.access(), &key, request, now())
            .await
    })
}

/// A Kill under the tag the person last read, on its own task.
fn kill(c: &Arc<Case>, fleet: &Uuid7, tag: String) -> JoinHandle<Result<SavedFleet>> {
    let (c, fleet) = (Arc::clone(c), fleet.clone());
    tokio::spawn(async move {
        let killed = Some("killed");
        let access = c.base.access();
        c.creations
            .patch(access, &fleet, Some(&tag), killed, None, now())
            .await
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_kill_committed_while_activation_waits_refuses_that_activation() {
    let c = Arc::new(Case::create().await);
    let Installing {
        fleet,
        key,
        request,
        tag,
        draft,
    } = Installing::save(&c, "kill-first").await;
    let wallet = Hold::take(&c.base.owner_url(), HOLD_WALLET, c.base.tenant.as_str()).await;
    let activation = activate(&c, key, request);
    blocked(&c, &[wallet.pid], AT_WALLET).await;
    let killed = bounded(
        "Kill never waits on a pending activation",
        kill(&c, &fleet, tag),
    )
    .await
    .expect("Kill task")
    .expect("Kill wins the fleet row");
    assert_eq!(killed.status, "killed");
    wallet.release().await;
    let refused = bounded("activation resumes", activation)
        .await
        .expect("activation task")
        .expect("a refused activation still answers the retained fleet");
    assert_eq!(
        refused.status, "killed",
        "activation saw the committed Kill"
    );
    assert_eq!(refused.setup.state, "cancelled");
    assert_eq!(c.admissions(&fleet).await, NO_ADMISSIONS);
    let restored = c.base.drafts.read(c.base.access(), &draft).await;
    assert_eq!(restored.expect("draft").state, DraftState::Editing);
    Arc::into_inner(c).expect("tasks joined").cleanup().await;
}

/// A fixture trigger removal can queue this table lock between activation and
/// Kill. Force that ordering so lock observation must follow the whole chain.
async fn kill_after_table_lock(
    c: &Arc<Case>,
    fleet: &Uuid7,
    tag: String,
    activating: i32,
) -> JoinHandle<Result<SavedFleet>> {
    let mut table = PgConnection::connect(&c.base.owner_url())
        .await
        .expect("table holder connects");
    sqlx::query("BEGIN")
        .execute(&mut table)
        .await
        .expect("table transaction");
    let queued = tokio::spawn(async move {
        sqlx::query("LOCK TABLE fleet.fleets IN ACCESS EXCLUSIVE MODE")
            .execute(&mut table)
            .await
            .expect("queued table lock");
        sqlx::query("COMMIT")
            .execute(&mut table)
            .await
            .expect("release table lock");
    });
    blocked(c, &[activating], "LOCK TABLE").await;
    let killing = kill(c, fleet, tag);
    tokio::spawn(async move {
        let result = killing.await.expect("Kill task");
        queued.await.expect("table lock task");
        result
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_activation_committed_first_makes_the_concurrent_kill_stale() {
    let c = Arc::new(Case::create().await);
    let Installing {
        fleet,
        key,
        request,
        tag,
        draft,
    } = Installing::save(&c, "activation-first").await;
    let linked = Hold::take(&c.base.owner_url(), HOLD_DRAFT, draft.as_str()).await;
    let activation = activate(&c, key, request);
    let activating = blocked(&c, &[linked.pid], AT_CLOSE).await;
    let kill = kill_after_table_lock(&c, &fleet, tag, activating).await;
    blocked(&c, &[activating], AT_LOCK).await;
    linked.release().await;
    let activated = bounded("activation commits", activation)
        .await
        .expect("activation task")
        .expect("activation wins");
    assert_eq!(activated.status, "active");
    assert!(matches!(
        activated.start,
        StartOutcome::RunNow {
            state: StartState::Accepted,
            ..
        }
    ));
    let stale = bounded("Kill resumes", kill)
        .await
        .expect("Kill task")
        .expect_err("a Kill against the pre-activation tag is stale");
    assert_eq!(stale.code(), error_code::AUTHORING_STALE);
    assert_eq!(stale.etag(), Some(activated.etag.as_str()));
    let current = c.creations.read(c.base.access(), &fleet).await;
    assert_eq!(current.expect("fleet").status, "active");
    assert_eq!(c.admissions(&fleet).await.len(), 1);
    let closed = c.base.drafts.read(c.base.access(), &draft).await;
    assert_eq!(closed.expect("draft").state, DraftState::Closed);
    Arc::into_inner(c).expect("tasks joined").cleanup().await;
}
