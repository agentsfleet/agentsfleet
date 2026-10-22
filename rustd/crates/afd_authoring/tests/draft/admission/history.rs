//! Cutoff range scans and real contention retain one claim per new message.

mod latency;
mod waiting;

use super::{Fixture, claim, fixture::fund, mint, now, queue, recovery::unknown};
use afd_authoring::{
    Claimed, Drafts,
    types::{Accepted, Draft, SendMessage},
};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_runner::Runners;
use afd_wire::lease::AcceptedKinds;
use serde_json::Value;
use sqlx::{Connection as _, PgConnection};
use std::sync::Arc;
use std::time::Duration;
use tokio::{sync::Barrier, task::JoinSet};

const CALLERS: usize = 100;
const HISTORY_MAX: i64 = 1_000;
const HISTORY: [i64; 5] = [100, 200, 400, 800, HISTORY_MAX];
const BUFFER_BOUND: u64 = 8;
const CONTENTION_TIMEOUT: Duration = Duration::from_secs(5);
const CONTENTION_POLL: Duration = Duration::from_millis(10);
const MIN_WAITERS: i64 = 2;
const BLOCKED: &str = "WITH RECURSIVE queued(pid) AS (SELECT $1::int UNION SELECT a.pid FROM pg_stat_activity a JOIN queued q ON q.pid = ANY(pg_blocking_pids(a.pid))) SELECT count(*) - 1 FROM queued";
const CLONE_HISTORY: &str = "INSERT INTO authoring.attempts SELECT r.* FROM authoring.attempts a CROSS JOIN unnest($2::uuid[],$3::uuid[],$4::bigint[]) n(id,lease_id,sequence) CROSS JOIN LATERAL jsonb_populate_record(NULL::authoring.attempts,to_jsonb(a)||jsonb_build_object('id',n.id,'lease_id',n.lease_id,'message_sequence',n.sequence,'reserved_nanos',0)) r WHERE a.id=$1::uuid";
const CUTOFF: &str = "UPDATE authoring.drafts SET last_message_sequence=$2,cancelled_through_sequence=$2 WHERE id=$1::uuid";
const EXPLAIN: &str = const_format::concatcp!(
    "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ",
    afd_authoring::Drafts::CURRENT_ATTEMPT_QUERY
);

async fn history(f: &Fixture, draft: &Draft, lease: &Claimed) {
    let mut owner = PgConnection::connect(&f.owner_url())
        .await
        .expect("isolated history owner");
    let mut first = 2;
    for last in HISTORY {
        seed(&mut owner, draft, lease, first, last).await;
        let plan: Value = sqlx::query_scalar(EXPLAIN)
            .bind(draft.id.as_str())
            .bind("settled")
            .bind(last)
            .fetch_one(&mut owner)
            .await
            .expect("production range query plan");
        indexed(&plan, last);
        first = last + 1;
    }
}

pub(super) async fn seed(
    owner: &mut PgConnection,
    draft: &Draft,
    lease: &Claimed,
    first: i64,
    last: i64,
) {
    sqlx::query(CLONE_HISTORY)
        .bind(&lease.attempt_id)
        .bind(
            (first..=last)
                .map(|_| mint().to_string())
                .collect::<Vec<_>>(),
        )
        .bind(
            (first..=last)
                .map(|_| mint().to_string())
                .collect::<Vec<_>>(),
        )
        .bind((first..=last).collect::<Vec<_>>())
        .execute(&mut *owner)
        .await
        .expect("retained historical attempts");
    sqlx::query(CUTOFF)
        .bind(draft.id.as_str())
        .bind(last)
        .execute(&mut *owner)
        .await
        .expect("historical accepted watermark");
    sqlx::query("ANALYZE authoring.attempts")
        .execute(&mut *owner)
        .await
        .expect("history statistics");
}

fn indexed(plan: &Value, last: i64) {
    let root = plan
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("Plan"))
        .expect("query plan root");
    assert!(
        plan.to_string()
            .contains("uq_attempts_draft_id_message_sequence"),
        "{plan}"
    );
    assert!(plan.to_string().contains("message_sequence >"), "{plan}");
    assert_eq!(root.get("Actual Rows").and_then(Value::as_f64), Some(0.0));
    let buffers = root
        .get("Shared Hit Blocks")
        .and_then(Value::as_u64)
        .expect("buffer counter");
    assert!(
        buffers <= BUFFER_BOUND,
        "history={last}, buffers={buffers}: {plan}"
    );
    eprintln!("retained history={last}: indexed cutoff; shared buffers={buffers}/{BUFFER_BOUND}");
}

async fn sends(f: Arc<Fixture>, id: &Uuid7, peer: Drafts) {
    let mut monitor = PgConnection::connect(&f.owner_url())
        .await
        .expect("independent lock monitor");
    let mut holder = f.connection().await;
    let mut held = holder
        .begin()
        .await
        .expect("hold draft for deterministic contention");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .expect("holder backend");
    sqlx::query("SELECT id FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE")
        .bind(id.as_str())
        .execute(&mut *held)
        .await
        .expect("lock the owned draft");
    // Two independently constructed services keep the database lock, rather
    // than a shared in-process queue, responsible for their common sequence.
    let mut tasks = concurrent_sends(&f, id, &[f.drafts.clone(), peer]);
    wait_for_contention(&mut monitor, pid).await;
    held.commit().await.expect("release concurrent senders");
    drop(holder);
    monitor
        .close()
        .await
        .expect("release the independent monitor");
    let mut sequences = Vec::new();
    while let Some(sent) = tasks.join_next().await {
        sequences.push(
            sent.expect("Send task")
                .sequence
                .parse::<i64>()
                .expect("decimal sequence"),
        );
    }
    sequences.sort_unstable();
    let last = HISTORY_MAX + i64::try_from(CALLERS).expect("caller count");
    assert_eq!(sequences, (HISTORY_MAX + 1..=last).collect::<Vec<_>>());
    eprintln!(
        "{CALLERS} concurrent Send callers; at least {MIN_WAITERS} database waiters; unique contiguous sequences"
    );
}

fn concurrent_sends(f: &Arc<Fixture>, id: &Uuid7, peers: &[Drafts; 2]) -> JoinSet<Accepted> {
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut tasks = JoinSet::new();
    for index in 0..CALLERS {
        let (f, id, barrier) = (Arc::clone(f), id.clone(), Arc::clone(&barrier));
        let drafts = peers
            .get(index % peers.len())
            .expect("service peer")
            .clone();
        tasks.spawn(async move {
            barrier.wait().await;
            let input = SendMessage {
                message: "A concurrent follow-up".into(),
                operation_id: mint(),
            };
            drafts
                .send(f.access(), &id, input, now())
                .await
                .expect("concurrent Send")
        });
    }
    tasks
}

async fn wait_for_contention(monitor: &mut PgConnection, pid: i32) {
    tokio::time::timeout(CONTENTION_TIMEOUT, async {
        loop {
            let blocked: i64 = sqlx::query_scalar(BLOCKED)
                .bind(pid)
                .fetch_one(&mut *monitor)
                .await
                .expect("lock census");
            if blocked >= MIN_WAITERS {
                break;
            }
            tokio::time::sleep(CONTENTION_POLL).await;
        }
    })
    .await
    .expect("two senders reached the database concurrently");
}

async fn claims(f: Arc<Fixture>, draft: &Draft) {
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut tasks = JoinSet::new();
    for _ in 0..CALLERS {
        let (f, draft, barrier) = (Arc::clone(&f), draft.clone(), Arc::clone(&barrier));
        tasks.spawn(async move {
            barrier.wait().await;
            claim(&f, &draft).await
        });
    }
    let mut admitted = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Some(lease) = result.expect("claim task") {
            admitted.push(lease);
        }
    }
    assert_eq!(admitted.len(), 1);
    assert_eq!(
        admitted.first().expect("single attempt").sequence,
        (HISTORY_MAX + 1).to_string()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authoring.attempts WHERE draft_id=$1::uuid")
            .bind(draft.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("retained and current attempts");
    assert_eq!(count, HISTORY_MAX + 1);
    eprintln!(
        "{CALLERS} concurrent claim callers; exactly one new attempt; all {HISTORY_MAX} older attempts retained"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres"]
async fn stopped_history_uses_sequence_index_and_concurrent_sends_keep_unique_sequences() {
    let f = Arc::new(fund(Fixture::isolated().await, 2).await);
    let draft = queue(&f, "Original unknown request").await;
    let lease = claim(&f, &draft).await.expect("first attempt");
    unknown(&f, &lease).await;
    f.drafts
        .cancel(f.access(), &draft.id, 1)
        .await
        .expect("Stop old work");
    history(&f, &draft, &lease).await;
    let peer = Drafts::new(
        f.db.clone(),
        Entropy::new(),
        crate::draft::fixture::ledger(&f.db),
    );
    sends(Arc::clone(&f), &draft.id, peer).await;
    claims(Arc::clone(&f), &draft).await;
    Arc::try_unwrap(f)
        .ok()
        .expect("all callers released fixture")
        .cleanup()
        .await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn discovery_offers_new_work_after_stopped_unknown_usage() {
    let f = fund(Fixture::isolated().await, 2).await;
    let draft = queue(&f, "Original unknown request").await;
    let lease = claim(&f, &draft).await.expect("first attempt");
    unknown(&f, &lease).await;
    f.drafts
        .cancel(f.access(), &draft.id, 1)
        .await
        .expect("Stop");
    f.drafts
        .send(
            f.access(),
            &draft.id,
            SendMessage {
                message: "Try the next request".into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("new message");
    let offer = Runners::new(f.db.clone(), Entropy::new())
        .claim_offer(&f.runner, AcceptedKinds::ALL)
        .await
        .expect("host poll")
        .expect("eligible host");
    let next = f
        .drafts
        .claim_offered(&f.runner, offer)
        .await
        .expect("discovery and admission")
        .expect("new work offered");
    assert_eq!(next.draft_id, draft.id.to_string());
    assert_eq!(next.sequence, "2");
    f.cleanup().await;
}
