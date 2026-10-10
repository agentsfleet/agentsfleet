//! Dimension 4.3 — what a superseded holder may write, which is nothing.
//!
//! Split from the other §4 suites because the precondition differs in the way
//! that matters: these need a lease whose FENCE can be compared against the
//! fleet's live sequence, and a store whose contents are read back after a
//! refusal. A suite that seeded that for every case would make the mint tests
//! depend on state they do not care about.
//!
//! # Why the refusal is proven by reading the store, not by the error alone
//!
//! `Plane::capture` refuses a stale token before it calls `Memories::capture`,
//! so the error and the empty write come from the same `return`. That makes the
//! error easy to assert and the important half easy to skip: a future edit that
//! moved the fence check BELOW the write would still return the same refusal,
//! and the only thing that would notice is a test which asks the store what it
//! holds afterwards. Every case here does.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::queue;
use crate::report_seed;
use std::borrow::Cow;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_memory::Memories;
use afd_wire::memory::{MemoryDelta, MemoryPushRequest, MemoryRecallRequest};

use self::report_seed::held;

/// The key both writes below use.
///
/// ONE key, deliberately: `(key, fleet_id)` is the upsert's conflict target, so
/// a second write under the same key OVERWRITES. That is what makes "the store
/// is unchanged" a real assertion rather than a count that would pass while the
/// content had been replaced.
const KEY: &str = "what-the-run-learned";

/// What the legitimate holder stores.
const HELD_CONTENT: &str = "the finding the current holder wrote";

/// What the superseded holder tries to store over it.
const SUPERSEDED_CONTENT: &str = "the finding a reclaimed holder must not write";

/// The retention category both entries carry.
const CATEGORY: &str = "core";

/// How many entries a recall asks for; any value in range serves.
const RECALL_LIMIT: usize = 10;

/// Dimension 4.3 — a fence below the fleet's live sequence writes nothing.
///
/// The positive control comes first and is not decoration: without a stored
/// entry to overwrite, a refused write and a write that silently did nothing
/// look identical from the store's side.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_fencing() {
    let run = held().await;
    let fleet_id = Uuid7::parse(&run.fleet).expect("the fixture id is a v7 spelling");
    let plane = run.fixtures.plane();
    let lease = run.issued.lease_id.as_str();

    // ── The current holder writes ───────────────────────────────────────────
    let stored = plane
        .capture(
            &run.runner,
            &fleet_id,
            &push(lease, run.fence.as_u64(), HELD_CONTENT),
            run.now,
        )
        .await
        .expect("the holder of the live fence may write");
    assert_eq!(
        stored.stored, 1,
        "one delta in, one entry stored — the control the refusal below is measured against"
    );

    // ── A token that is not the lease's own is refused ──────────────────────
    // One below the lease's own token, which is still the live sequence: the
    // fence refuses it as not the holder's. A reclaim that moved the fleet past
    // the lease is `test_memory_routes_refuse_a_superseded_active_lease`.
    let superseded = run.fence.as_u64() - 1;
    let refusal = plane
        .capture(
            &run.runner,
            &fleet_id,
            &push(lease, superseded, SUPERSEDED_CONTENT),
            run.now,
        )
        .await
        .expect_err("a token below the live sequence cannot write");
    assert_eq!(
        refusal.code(),
        error_code::RUN_STALE_FENCING_TOKEN,
        "the runner is told WHICH guard refused: the fence, not the lease's \
         ownership and not its expiry"
    );

    // ── And the store is exactly as the holder left it ──────────────────────
    let window = plane
        .hydrate(&run.runner, &fleet_id, run.now)
        .await
        .expect("the holder may read its fleet's memory");
    assert_eq!(
        window.memory.len(),
        1,
        "the refused write added nothing — the fence is checked BEFORE the \
         upsert, not after it"
    );
    let only = window
        .memory
        .first()
        .expect("the window carries the holder's entry");
    assert_eq!(
        only.content.as_ref(),
        HELD_CONTENT,
        "and overwrote nothing: the entry still reads as the legitimate holder \
         wrote it, which a same-key upsert past the guard would have replaced"
    );
    assert_eq!(
        only.key.as_ref(),
        KEY,
        "under the key both writes named, which is what makes the assertion \
         above an overwrite check rather than a count"
    );

    queue::clear_ready(run.fixtures.queue(), &run.fleet).await;
    run.fixtures.cleanup().await;
}

/// A token that is not the lease's own writes nothing, however high it is.
///
/// A lease is issued only while the fleet's sequence equals its token, and the
/// sequence only grows, so the holder's token is never above the live one. A
/// token that is ABOVE it is not a holder racing a reclaim; it is a caller
/// that does not hold the lease's token, and `u64::MAX` is the value that once
/// passed a check written as "not below".
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_refuses_a_token_that_is_not_the_holders() {
    let run = held().await;
    let fleet_id = Uuid7::parse(&run.fleet).expect("the fixture id is a v7 spelling");
    let plane = run.fixtures.plane();
    let lease = run.issued.lease_id.as_str();

    for presented in [run.fence.as_u64() + 1, u64::MAX] {
        let refusal = plane
            .capture(
                &run.runner,
                &fleet_id,
                &push(lease, presented, SUPERSEDED_CONTENT),
                run.now,
            )
            .await
            .expect_err("only the lease's own token may write");
        assert_eq!(
            refusal.code(),
            error_code::RUN_STALE_FENCING_TOKEN,
            "token {presented} is refused by the fence, not by ownership or expiry"
        );
    }
    assert!(
        stored_window(&run, &fleet_id).await.is_empty(),
        "and nothing was written on the way to refusing either token"
    );

    queue::clear_ready(run.fixtures.queue(), &run.fleet).await;
    run.fixtures.cleanup().await;
}

/// A lease the fleet has moved past reaches none of its memory, while the row
/// still reads `active`.
///
/// A won claim bumps the fleet's sequence in one statement and expires the
/// prior lease in the next (`Plane::take_claimed`); a claimer that dies between
/// the two leaves the old row live and active until its own expiry. The bump
/// is written directly here, because the claim path closes the window too fast
/// to observe, and every call is made inside the old lease's window so each
/// refusal comes from the fence and none from expiry.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_routes_refuse_a_superseded_active_lease() {
    let run = held().await;
    let fleet_id = Uuid7::parse(&run.fleet).expect("the fixture id is a v7 spelling");
    let plane = run.fixtures.plane();
    let lease = run.issued.lease_id.as_str();
    // The control: while the fleet has not moved past it, the holder recalls
    // and hydrates, so each refusal below is the reclaim's doing.
    plane
        .recall(
            &run.runner,
            &fleet_id,
            &recall(lease, run.fence.as_u64()),
            run.now,
        )
        .await
        .expect("the holder recalls while the fleet has not moved past it");
    plane
        .hydrate(&run.runner, &fleet_id, run.now)
        .await
        .expect("and hydrates");
    let live = supersede(&run).await;

    for presented in [run.fence.as_u64(), live, u64::MAX] {
        let pushed = plane
            .capture(
                &run.runner,
                &fleet_id,
                &push(lease, presented, SUPERSEDED_CONTENT),
                run.now,
            )
            .await
            .expect_err("a superseded lease cannot write");
        assert_eq!(
            pushed.code(),
            error_code::RUN_STALE_FENCING_TOKEN,
            "capture with {presented}"
        );

        let recalled = plane
            .recall(&run.runner, &fleet_id, &recall(lease, presented), run.now)
            .await
            .expect_err("a superseded lease cannot recall");
        assert_eq!(
            recalled.code(),
            error_code::RUN_STALE_FENCING_TOKEN,
            "recall with {presented}"
        );
    }
    let hydrated = plane
        .hydrate(&run.runner, &fleet_id, run.now)
        .await
        .expect_err("a superseded lease cannot hydrate");
    assert_eq!(hydrated.code(), error_code::RUN_STALE_FENCING_TOKEN);
    assert!(
        stored_window(&run, &fleet_id).await.is_empty(),
        "no refused capture reached the store"
    );

    queue::clear_ready(run.fixtures.queue(), &run.fleet).await;
    run.fixtures.cleanup().await;
}

/// Dimension 4.3 — a lease that is not this runner's writes into no fleet.
///
/// The fence is only half of capture's authorization; the other half is the
/// lease's OWNERSHIP, and both live in the same statement's `WHERE`. Proven
/// here because a refusal that came from the fence would look identical from
/// the caller's side if the ownership predicate were dropped — the spare runner
/// holds no lease at all, so a capture in its name must find nothing rather
/// than be fenced.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_refuses_a_lease_the_runner_does_not_hold() {
    let run = held().await;
    let fleet_id = Uuid7::parse(&run.fleet).expect("the fixture id is a v7 spelling");
    let plane = run.fixtures.plane();

    let refusal = plane
        .capture(
            &run.spare,
            &fleet_id,
            &push(
                run.issued.lease_id.as_str(),
                run.fence.as_u64(),
                SUPERSEDED_CONTENT,
            ),
            run.now,
        )
        .await
        .expect_err("a runner cannot write memory under another runner's lease");
    assert_eq!(
        refusal.code(),
        error_code::RUN_LEASE_NOT_FOUND,
        "not found, not fenced: presenting a correct token for a lease you do \
         not hold is not a stale-holder problem"
    );

    let window = plane
        .hydrate(&run.runner, &fleet_id, run.now)
        .await
        .expect("the real holder may read");
    assert!(
        window.memory.is_empty(),
        "and nothing was written on the way to refusing it"
    );

    queue::clear_ready(run.fixtures.queue(), &run.fleet).await;
    run.fixtures.cleanup().await;
}

/// The fleet's stored window, read from the store rather than through a fenced
/// verb, so a refusal cannot hide what it let through.
async fn stored_window(run: &report_seed::Held, fleet_id: &Uuid7) -> Vec<String> {
    Memories::new(run.fixtures.database.clone(), Entropy::new())
        .hydrate(fleet_id)
        .await
        .expect("the store answers")
        .memory
        .into_iter()
        .map(|entry| entry.content.into_owned())
        .collect()
}

/// Moves the fleet's live sequence one past the held lease, as a won claim
/// does before it expires the prior lease, and answers the new sequence.
async fn supersede(run: &report_seed::Held) -> u64 {
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let live: i64 = sqlx::query_scalar(
        "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1
         WHERE fleet_id = $1::uuid RETURNING fencing_seq",
    )
    .bind(&run.fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("the held fleet has an affinity row");
    assert!(
        live > run.fence.as_i64(),
        "the sequence must outrank the held lease, or this test proves nothing"
    );
    u64::try_from(live).expect("a sequence is never negative")
}

/// One recall body, matching every entry.
fn recall(lease_id: &str, fencing_token: u64) -> MemoryRecallRequest<'_> {
    MemoryRecallRequest {
        lease_id: Cow::Borrowed(lease_id),
        fencing_token,
        query: Cow::Borrowed(""),
        limit: RECALL_LIMIT,
    }
}

/// One capture body, over one delta.
///
/// A builder because every case sends the same shape and differs only in the
/// token and the content — the two fields a literal per call site would let
/// drift apart, and the exact pair every assertion above turns on.
fn push<'a>(lease_id: &'a str, fencing_token: u64, content: &'a str) -> MemoryPushRequest<'a> {
    MemoryPushRequest {
        lease_id: Cow::Borrowed(lease_id),
        fencing_token,
        memory: vec![MemoryDelta {
            key: Cow::Borrowed(KEY),
            content: Cow::Borrowed(content),
            category: Cow::Borrowed(CATEGORY),
            visibility: afd_wire::memory::Visibility::Fleet,
        }],
    }
}
