//! A draft assignment claims without a lock on the draft or its creator: the
//! thread claim and the turn claim decide, each one statement, so a reader or
//! a writer of the draft never makes a pass skip it, and anything that moved
//! the draft since the pass read it rolls the pass back.
//!
//! Each proof parks the pass, or the work it races, on a row or table another
//! transaction holds, waits until the lock graph shows it queued, and only
//! then lets it go. The holds stall every reader of that row or table, so each
//! case owns its database (`Case::queued`).

use std::time::Duration;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::run::AcceptedKinds;
use afd_db::{Db, config::DbRole};
use afd_fleet::run::draft::DraftAssignment;
use afd_setup::types::{DraftState, MessageQuery};
use sqlx::pool::PoolConnection;

use super::Case;
use super::placement::tenant_runner;

/// How long a proof waits for the raced work to queue behind its holder. The
/// API login's lock bound ends a wait first, at 2 s.
const QUEUE_LIMIT: Duration = Duration::from_secs(5);

/// How many backends wait, directly or behind another waiter, on `$1`.
const QUEUED: &str = "WITH RECURSIVE queued(pid) AS (SELECT $1::int UNION \
     SELECT a.pid FROM pg_stat_activity a JOIN queued q \
     ON q.pid = ANY(pg_blocking_pids(a.pid))) SELECT count(*) - 1 FROM queued";

/// The table the payer's balance gate reads, after the pass read the draft
/// and its head and before it claims anything.
const LOCK_WALLETS: &str = "LOCK TABLE billing.wallets IN ACCESS EXCLUSIVE MODE";

/// The table a draft's message page reads its usage from, after the draft,
/// and that no assignment reads.
const LOCK_USAGE: &str = "LOCK TABLE billing.usage_lines IN ACCESS EXCLUSIVE MODE";

/// The draft's head turn, share-held so the turn claim waits on it after the
/// thread claim won.
const HOLD_TURN: &str = "SELECT pg_backend_pid() FROM thread.turns WHERE thread_id = $1 FOR SHARE";

/// The draft's thread, held so the thread claim, and a deletion's cascade,
/// wait on it.
const HOLD_THREAD: &str =
    "SELECT pg_backend_pid() FROM thread.threads WHERE id = $1 FOR NO KEY UPDATE";

/// A transaction holding a row or a table, and its backend.
struct Holder {
    connection: PoolConnection<sqlx::Postgres>,
    pid: i32,
}

impl Holder {
    /// Opens a transaction on `database` that runs `statement`, a table lock.
    async fn table(database: &Db, statement: &'static str) -> Self {
        let mut holder = Self::open(database).await;
        sqlx::query(statement)
            .execute(&mut *holder.connection)
            .await
            .expect("the holder takes the table");
        holder
    }

    /// Opens a transaction on `database` that holds `row`'s rows through
    /// `statement`, which answers the backend's pid once per row it holds.
    async fn rows(database: &Db, statement: &'static str, row: &Uuid7) -> Self {
        let mut holder = Self::open(database).await;
        let held: Vec<i32> = sqlx::query_scalar(statement)
            .bind(row)
            .fetch_all(&mut *holder.connection)
            .await
            .expect("the holder takes the rows");
        assert!(!held.is_empty(), "the holder holds a row: {statement}");
        holder
    }

    async fn open(database: &Db) -> Self {
        let mut connection = database.acquire().await.expect("a holder connection");
        sqlx::query("BEGIN")
            .execute(&mut *connection)
            .await
            .expect("the holder opens");
        let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *connection)
            .await
            .expect("the holder's backend");
        Self { connection, pid }
    }

    async fn release(mut self) {
        sqlx::query("ROLLBACK")
            .execute(&mut *self.connection)
            .await
            .expect("the holder releases");
    }
}

impl Case {
    /// A pool as the lane's owner, which may take a table lock.
    async fn owner(&self) -> Db {
        self.running.fixtures.open(DbRole::Migrator).await
    }

    /// Waits until `waiting` backends queue behind the backend `holder`.
    async fn queued_behind(&self, holder: i32, waiting: i64) {
        tokio::time::timeout(QUEUE_LIMIT, async {
            let mut observer = self.connection().await;
            loop {
                let queued: i64 = sqlx::query_scalar(QUEUED)
                    .bind(holder)
                    .fetch_one(&mut *observer)
                    .await
                    .expect("the lock graph reads");
                if queued >= waiting {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the raced work queues behind the holder");
    }

    /// One draft pass of `runner`'s, through the production draft assignment.
    async fn pass(&self, runner: &Uuid7) -> Option<DraftAssignment> {
        let fixtures = &self.running.fixtures;
        let ticket = afd_runner::Runners::new(fixtures.database.clone(), Entropy::new())
            .assignment_ticket(runner, AcceptedKinds::ALL)
            .await
            .expect("the ticket reads")
            .expect("the host has a placement choice");
        fixtures
            .plane()
            .assign_draft(&self.drafts, runner, ticket, self.running.now)
            .await
            .expect("a pass answers, whatever moved under it")
    }

    /// The fixture host's draft pass.
    async fn host_pass(&self) -> Option<DraftAssignment> {
        self.pass(&self.running.runner).await
    }

    /// The draft's thread fence, `None` once the thread is gone.
    async fn fence(&self) -> Option<i64> {
        sqlx::query_scalar("SELECT fencing_token FROM thread.threads WHERE id = $1")
            .bind(&self.draft.id)
            .fetch_optional(&mut *self.connection().await)
            .await
            .expect("the draft's thread")
    }

    /// The draft's one turn.
    async fn turn_id(&self) -> Uuid7 {
        sqlx::query_scalar("SELECT id FROM thread.turns WHERE thread_id = $1")
            .bind(&self.draft.id)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the draft's one turn")
    }

    /// The draft's active runs.
    async fn active_runs(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE draft_id = $1 AND status = $2")
            .bind(&self.draft.id)
            .bind(afd_state::sql::RUN_STATUS_ACTIVE)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the draft's runs")
    }
}

// Value: protects=two passes over one draft, on two hosts, issue one run: the
// thread claim decides, and the pass that loses it answers no assignment
// rather than a fault; fails_when=assign_thread.sql's live-run condition goes,
// or a losing pass errors; why_new=the draft lock that once excluded a second
// pass is gone; seam=assign_draft on two hosts over a private live Postgres,
// both past their reads before either commits.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_passes_issue_one_run() {
    let case = Case::queued().await;
    let second = tenant_runner(&case.running.fixtures, &case.seat.tenant).await;
    let database = &case.running.fixtures.database;
    let holder = Holder::rows(database, HOLD_TURN, &case.draft.id).await;
    let pid = holder.pid;

    // The first pass to win the thread waits on the held turn; the other waits
    // on the first's thread.
    let (first, other, ()) = tokio::join!(case.host_pass(), case.pass(&second), async {
        case.queued_behind(pid, 2).await;
        holder.release().await;
    });

    let issued = [&first, &other]
        .iter()
        .filter(|placed| placed.is_some())
        .count();
    assert_eq!(issued, 1, "exactly one pass issues the run");
    assert_eq!(
        case.runs().await,
        [(1, "active".to_owned())],
        "one run, fence 1"
    );
    assert_eq!(case.fence().await, Some(1), "one fence bump survives");
    assert_eq!(case.turn_on_head().await.status, "running");
    case.cleanup().await;
}

// Value: protects=a head a Stop cancelled after the pass read it is not
// issued: the turn claim misses, and the pass rolls back its fence bump;
// fails_when=running.sql stops testing that the turn is unsettled and queued
// or running, and a cancelled turn is run again; why_new=the claim ignored its
// row count; seam=assign_draft parked at its balance gate while the
// production Stop commits, over a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stopped_head_is_not_issued() {
    let case = Case::queued().await;
    let turn = case.turn_id().await;
    let fence = case.fence().await;
    let owner = case.owner().await;
    let holder = Holder::table(&owner, LOCK_WALLETS).await;
    let pid = holder.pid;

    let (placed, ()) = tokio::join!(case.host_pass(), async {
        case.queued_behind(pid, 1).await;
        let stopped = case
            .drafts
            .stop_turn(case.seat.access(), &case.draft.id, &turn)
            .await
            .expect("the Stop answers");
        assert!(stopped.is_some(), "the Stop finds the turn");
        holder.release().await;
    });

    assert!(placed.is_none(), "a stopped head is not issued");
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    assert_eq!(case.turn_on_head().await.status, "cancelled");
    assert_eq!(case.fence().await, fence, "the fence bump rolled back");
    assert!(case.host_pass().await.is_none(), "nothing is left to run");
    owner.close().await;
    case.cleanup().await;
}

// Value: protects=a draft that left editing after the pass read it is not
// issued; fails_when=running.sql stops testing the draft's status in the
// statement that claims its turn; why_new=the draft lock that once excluded a
// status change during the pass is gone; seam=assign_draft parked at its
// balance gate while the draft moves to saving, over a private live Postgres.
// No production writer moves a draft with a queued turn (each checks the
// draft idle first), so the move is written directly.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_that_left_editing_is_not_issued() {
    let case = Case::queued().await;
    let fence = case.fence().await;
    let owner = case.owner().await;
    let holder = Holder::table(&owner, LOCK_WALLETS).await;
    let pid = holder.pid;

    let (placed, ()) = tokio::join!(case.host_pass(), async {
        case.queued_behind(pid, 1).await;
        let moved = sqlx::query("UPDATE setup.drafts SET status = $2 WHERE id = $1")
            .bind(&case.draft.id)
            .bind(DraftState::Saving.as_str())
            .execute(&mut *case.connection().await)
            .await
            .expect("the draft leaves editing");
        assert_eq!(moved.rows_affected(), 1);
        holder.release().await;
    });

    assert!(placed.is_none(), "a draft that left editing is not issued");
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    assert_eq!(
        case.turn_on_head().await.status,
        "queued",
        "the turn is left"
    );
    assert_eq!(case.fence().await, fence, "the fence bump rolled back");
    owner.close().await;
    case.cleanup().await;
}

// Value: protects=a draft deleted while a pass holds its thread leaves no
// active run: the deletion waits out the pass, then deletes the run it issued;
// fails_when=privacy.rs stops holding the thread before its deletes, and the
// issued run outlives its draft or the two deadlock; why_new=the draft lock
// that once ordered the two is gone; seam=assign_draft parked at
// its turn claim while the production delete queues behind it, over a
// private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_deleted_draft_leaves_no_active_run() {
    let case = Case::queued().await;
    let database = &case.running.fixtures.database;
    let holder = Holder::rows(database, HOLD_TURN, &case.draft.id).await;
    let pid = holder.pid;

    let (placed, deleted, ()) = tokio::join!(
        case.host_pass(),
        async {
            case.queued_behind(pid, 1).await;
            case.drafts.delete(case.seat.access(), &case.draft.id).await
        },
        async {
            case.queued_behind(pid, 2).await;
            holder.release().await;
        }
    );

    assert!(
        placed.is_some(),
        "the pass that held the thread issued its run"
    );
    deleted.expect("the deletion commits after it");
    assert_eq!(
        case.active_runs().await,
        0,
        "the issued run went with its draft"
    );
    assert_eq!(case.fence().await, None, "the thread went with its draft");
    case.cleanup().await;
}

// Value: protects=a draft deleted between a pass's reads and its thread claim
// is skipped: the claim's insert names a draft that is gone, and the pass
// reads that foreign-key refusal as no assignment, never as a failed pass;
// fails_when=take.rs stops matching threads_draft_id_fkey and the pass
// errors; why_new=the branch is new; seam=the production delete and
// assign_draft queued in that order on a held thread, over a private live
// Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_deleted_before_its_claim_is_skipped() {
    let case = Case::queued().await;
    let database = &case.running.fixtures.database;
    let holder = Holder::rows(database, HOLD_THREAD, &case.draft.id).await;
    let pid = holder.pid;

    let (deleted, placed, ()) = tokio::join!(
        case.drafts.delete(case.seat.access(), &case.draft.id),
        async {
            case.queued_behind(pid, 1).await;
            case.host_pass().await
        },
        async {
            case.queued_behind(pid, 2).await;
            holder.release().await;
        }
    );

    deleted.expect("the deletion, queued first, commits");
    assert!(placed.is_none(), "the pass skips the deleted draft");
    assert_eq!(case.active_runs().await, 0, "no run written");
    assert_eq!(case.fence().await, None, "no thread recreated");
    case.cleanup().await;
}

// Value: protects=a Stop that waited on a pass's thread sees and marks the
// run that pass issued; fails_when=stop.rs stops holding the thread in its own
// statement, and Stop's statement, whose snapshot predates the issue, cancels
// the turn and leaves the run unmarked; why_new=the draft lock that once
// ordered the two is gone; seam=assign_draft parked at its turn claim while
// the production Stop queues behind it, over a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stop_queued_behind_an_issue_marks_the_issued_run() {
    let case = Case::queued().await;
    let turn = case.turn_id().await;
    let database = &case.running.fixtures.database;
    let holder = Holder::rows(database, HOLD_TURN, &case.draft.id).await;
    let pid = holder.pid;

    let (placed, stopped, ()) = tokio::join!(
        case.host_pass(),
        async {
            case.queued_behind(pid, 1).await;
            case.drafts
                .stop_turn(case.seat.access(), &case.draft.id, &turn)
                .await
        },
        async {
            case.queued_behind(pid, 2).await;
            holder.release().await;
        }
    );

    assert!(
        placed.is_some(),
        "the pass that held the thread issued its run"
    );
    let stopped = stopped
        .expect("the Stop answers")
        .expect("the Stop finds the turn");
    let afd_turns::view::stop::Stop::Cancelled { runs, running, .. } = stopped.stop else {
        unreachable_stop();
    };
    assert!(running, "the Stop saw the run serving the turn");
    assert_eq!(runs.len(), 1, "the Stop marked the issued run");
    let marked: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar("SELECT cancel_requested_at FROM thread.runs WHERE draft_id = $1")
            .bind(&case.draft.id)
            .fetch_one(&mut *case.connection().await)
            .await
            .expect("the issued run");
    assert!(marked.is_some(), "the issued run carries its cancel mark");
    assert_eq!(case.turn_on_head().await.status, "cancelled");
    case.cleanup().await;
}

// Value: protects=a page read in flight on the draft delays no assignment, so
// a Save queued behind the editor's own reads runs on the next pass (the Save
// that sat 64 s); fails_when=the message page takes the draft row lock again,
// or the assignment locks the draft, and the pass skips the draft as busy;
// why_new=the read took the writer lock and the pass refused it NOWAIT;
// seam=Drafts::messages parked after its draft read while assign_draft runs,
// over a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_read_in_flight_does_not_delay_its_assignment() {
    let case = Case::queued().await;
    let owner = case.owner().await;
    let holder = Holder::table(&owner, LOCK_USAGE).await;
    let pid = holder.pid;
    let page = MessageQuery {
        limit: 25,
        starting_after: None,
    };

    let (read, placed) = tokio::join!(
        case.drafts
            .messages(case.seat.access(), &case.draft.id, page),
        async {
            case.queued_behind(pid, 1).await;
            let placed = case.host_pass().await;
            holder.release().await;
            placed
        }
    );

    let placed = placed.expect("the pass issues the run while the read is in flight");
    let run: Uuid7 = sqlx::query_scalar("SELECT id FROM thread.runs WHERE draft_id = $1")
        .bind(&case.draft.id)
        .fetch_one(&mut *case.connection().await)
        .await
        .expect("the issued run");
    assert_eq!(placed.assigned().run_id, run);
    assert_eq!(case.runs().await, [(1, "active".to_owned())]);
    let read = read.expect("the read answers once the table is free");
    assert_eq!(
        read.items.len(),
        1,
        "the person's message, before any answer"
    );
    owner.close().await;
    case.cleanup().await;
}

#[expect(clippy::panic, reason = "a Stop of an unsettled turn cancels it")]
fn unreachable_stop() -> ! {
    panic!("the Stop did not cancel the turn")
}
