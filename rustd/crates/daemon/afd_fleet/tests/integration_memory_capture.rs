//! What a superseded holder may write, which is nothing.
//!
//! Split from the other memory suites because the precondition differs in the way
//! that matters: these need a run whose FENCE can be compared against the
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

use crate::report_seed;
use std::borrow::Cow;

use af_core::error_code;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::memory::{MemoryDelta, MemoryPushRequest, MemoryRecallRequest};
use afd_memory::Memories;

use self::report_seed::held;

/// The key both writes below use.
///
/// ONE key, deliberately: `(key, fleet_id)` is the upsert's conflict target, so
/// a second write under the same key OVERWRITES. That is what makes "the store
/// is unchanged" a real assertion rather than a count that would pass while the
/// content had been replaced.
const KEY: &str = "what-the-test_run-learned";

/// What the legitimate holder stores.
const HELD_CONTENT: &str = "the finding the current holder wrote";

/// What the superseded holder tries to store over it.
const SUPERSEDED_CONTENT: &str = "the finding a reassigned holder must not write";

/// The retention category both entries carry.
const CATEGORY: &str = "core";

/// How many entries a recall asks for; any value in range serves.
const RECALL_LIMIT: usize = 10;

/// A fence below the fleet's live sequence writes nothing.
///
/// The positive control comes first and is not decoration: without a stored
/// entry to overwrite, a refused write and a write that silently did nothing
/// look identical from the store's side.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_fencing() {
    let test_run = held().await;
    let fleet_id = test_run.fleet.clone();
    let plane = test_run.fixtures.plane();
    let run = test_run.issued.run_id.as_str();

    // ── The current holder writes ───────────────────────────────────────────
    let stored = plane
        .capture(
            &test_run.runner,
            &fleet_id,
            &push(run, test_run.fence.as_u64(), HELD_CONTENT),
            test_run.now,
        )
        .await
        .expect("the holder of the live fence may write");
    assert_eq!(
        stored.stored, 1,
        "one delta in, one entry stored — the control the refusal below is measured against"
    );

    // ── A token that is not the run's own is refused ──────────────────────
    // One below the run's own token, which is still the live sequence: the
    // fence refuses it as not the holder's. A reassignment that moved the fleet past
    // the run is `test_memory_routes_refuse_a_superseded_active_run`.
    let superseded = test_run.fence.as_u64() - 1;
    let refusal = plane
        .capture(
            &test_run.runner,
            &fleet_id,
            &push(run, superseded, SUPERSEDED_CONTENT),
            test_run.now,
        )
        .await
        .expect_err("a token below the live sequence cannot write");
    assert_eq!(
        refusal.code(),
        error_code::RUN_STALE_FENCING_TOKEN,
        "the runner is told WHICH guard refused: the fence, not the run's \
         ownership and not its expiry"
    );

    // ── And the store is exactly as the holder left it ──────────────────────
    let window = plane
        .load(&test_run.runner, &fleet_id)
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

    test_run.fixtures.cleanup().await;
}

/// A token that is not the run's own writes nothing, however high it is.
///
/// A run is issued only while the fleet's sequence equals its token, and the
/// sequence only grows, so the holder's token is never above the live one. A
/// token that is ABOVE it is not a holder racing a reassignment; it is a caller
/// that does not hold the run's token, and `u64::MAX` is the value that once
/// passed a check written as "not below".
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_refuses_a_token_that_is_not_the_holders() {
    let test_run = held().await;
    let fleet_id = test_run.fleet.clone();
    let plane = test_run.fixtures.plane();
    let run = test_run.issued.run_id.as_str();

    for presented in [test_run.fence.as_u64() + 1, u64::MAX] {
        let refusal = plane
            .capture(
                &test_run.runner,
                &fleet_id,
                &push(run, presented, SUPERSEDED_CONTENT),
                test_run.now,
            )
            .await
            .expect_err("only the run's own token may write");
        assert_eq!(
            refusal.code(),
            error_code::RUN_STALE_FENCING_TOKEN,
            "token {presented} is refused by the fence, not by ownership or expiry"
        );
    }
    assert!(
        stored_window(&test_run, &fleet_id).await.is_empty(),
        "and nothing was written on the way to refusing either token"
    );

    test_run.fixtures.cleanup().await;
}

/// A run the fleet has moved past reaches none of its memory, while the row
/// still reads `active`.
///
/// A won assignment bumps the fleet's sequence in one statement and expires the
/// prior run in the next (`expire_prior_run.sql`); a contender that dies between
/// the two leaves the old row live and active until its own expiry. The bump
/// is written directly here, because the assignment path closes the window too fast
/// to observe, and every call is made inside the old run's window so each
/// refusal comes from the fence and none from expiry.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_routes_refuse_a_superseded_active_run() {
    let test_run = held().await;
    let fleet_id = test_run.fleet.clone();
    let plane = test_run.fixtures.plane();
    let run = test_run.issued.run_id.as_str();
    // The control: while the fleet has not moved past it, the holder recalls
    // and loads, so each refusal below is the reassignment's doing.
    plane
        .recall(
            &test_run.runner,
            &fleet_id,
            &recall(run, test_run.fence.as_u64()),
        )
        .await
        .expect("the holder recalls while the fleet has not moved past it");
    plane
        .load(&test_run.runner, &fleet_id)
        .await
        .expect("and loads");
    let live = test_run.supersede().await;

    for presented in [test_run.fence.as_u64(), live, u64::MAX] {
        let pushed = plane
            .capture(
                &test_run.runner,
                &fleet_id,
                &push(run, presented, SUPERSEDED_CONTENT),
                test_run.now,
            )
            .await
            .expect_err("a superseded run cannot write");
        assert_eq!(
            pushed.code(),
            error_code::RUN_STALE_FENCING_TOKEN,
            "capture with {presented}"
        );

        let recalled = plane
            .recall(&test_run.runner, &fleet_id, &recall(run, presented))
            .await
            .expect_err("a superseded run cannot recall");
        assert_eq!(
            recalled.code(),
            error_code::RUN_STALE_FENCING_TOKEN,
            "recall with {presented}"
        );
    }
    let loaded = plane
        .load(&test_run.runner, &fleet_id)
        .await
        .expect_err("a superseded run cannot load");
    assert_eq!(loaded.code(), error_code::RUN_STALE_FENCING_TOKEN);
    assert!(
        stored_window(&test_run, &fleet_id).await.is_empty(),
        "no refused capture reached the store"
    );

    test_run.fixtures.cleanup().await;
}

/// A run that is not this runner's writes into no fleet.
///
/// The fence is only half of capture's authorization; the other half is the
/// run's OWNERSHIP, and both live in the same statement's `WHERE`. Proven
/// here because a refusal that came from the fence would look identical from
/// the caller's side if the ownership predicate were dropped — the spare runner
/// holds no run at all, so a capture in its name must find nothing rather
/// than be fenced.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_refuses_a_run_the_runner_does_not_hold() {
    let test_run = held().await;
    let fleet_id = test_run.fleet.clone();
    let plane = test_run.fixtures.plane();

    let refusal = plane
        .capture(
            &test_run.spare,
            &fleet_id,
            &push(
                test_run.issued.run_id.as_str(),
                test_run.fence.as_u64(),
                SUPERSEDED_CONTENT,
            ),
            test_run.now,
        )
        .await
        .expect_err("a runner cannot write memory under another runner's run");
    assert_eq!(
        refusal.code(),
        error_code::RUN_NOT_FOUND,
        "not found, not fenced: presenting a correct token for a run you do \
         not hold is not a stale-holder problem"
    );

    let window = plane
        .load(&test_run.runner, &fleet_id)
        .await
        .expect("the real holder may read");
    assert!(
        window.memory.is_empty(),
        "and nothing was written on the way to refusing it"
    );

    test_run.fixtures.cleanup().await;
}

/// The fleet's stored window, read from the store rather than through a fenced
/// verb, so a refusal cannot hide what it let through.
async fn stored_window(test_run: &report_seed::Held, fleet_id: &Uuid7) -> Vec<String> {
    Memories::new(test_run.fixtures.database.clone(), Entropy::new())
        .load(fleet_id)
        .await
        .expect("the store answers")
        .memory
        .into_iter()
        .map(|entry| entry.content.into_owned())
        .collect()
}

/// One recall body, matching every entry.
const fn recall(run_id: &str, fencing_token: u64) -> MemoryRecallRequest<'_> {
    MemoryRecallRequest {
        run_id: Cow::Borrowed(run_id),
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
fn push<'a>(run_id: &'a str, fencing_token: u64, content: &'a str) -> MemoryPushRequest<'a> {
    MemoryPushRequest {
        run_id: Cow::Borrowed(run_id),
        fencing_token,
        memory: vec![MemoryDelta {
            key: Cow::Borrowed(KEY),
            content: Cow::Borrowed(content),
            category: Cow::Borrowed(CATEGORY),
            visibility: af_wire::memory::Visibility::Fleet,
        }],
    }
}
