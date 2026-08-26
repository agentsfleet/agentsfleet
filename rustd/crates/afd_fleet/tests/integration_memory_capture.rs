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

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_report_reads.rs"]
mod report_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_report_seed.rs"]
mod report_seed;

use std::borrow::Cow;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::memory::{MemoryDelta, MemoryPushRequest};

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

    // ── A superseded holder is refused ──────────────────────────────────────
    // One below the live sequence, which is exactly what a holder carries after
    // a reclaim has moved the fleet on without it.
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
        window.len(),
        1,
        "the refused write added nothing — the fence is checked BEFORE the \
         upsert, not after it"
    );
    let only = window
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

/// Dimension 4.3 — a token ABOVE the live sequence is admitted, not refused.
///
/// The other side of `<`, and the reason the comparison is not `!=`. A fleet's
/// sequence is bumped by the reclaim that issues the next lease, so a holder
/// can legitimately present a token the affinity row has not caught up to.
/// Refusing that would make the guard reject the CURRENT holder every time a
/// reclaim raced its write — the failure mode a stricter-looking check invites.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_memory_capture_admits_a_token_above_the_live_sequence() {
    let run = held().await;
    let fleet_id = Uuid7::parse(&run.fleet).expect("the fixture id is a v7 spelling");
    let plane = run.fixtures.plane();

    let ahead = run.fence.as_u64() + 1;
    let stored = plane
        .capture(
            &run.runner,
            &fleet_id,
            &push(run.issued.lease_id.as_str(), ahead, HELD_CONTENT),
            run.now,
        )
        .await
        .expect("a holder ahead of the recorded sequence still holds the lease");
    assert_eq!(
        stored.stored, 1,
        "the guard refuses what is BELOW the live sequence and nothing else"
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
        window.is_empty(),
        "and nothing was written on the way to refusing it"
    );

    queue::clear_ready(run.fixtures.queue(), &run.fleet).await;
    run.fixtures.cleanup().await;
}

/// One capture body, over one delta.
///
/// A builder because three cases send the same shape and differ only in the
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
        }],
    }
}
