//! Length bounds on the strings that identify a model.
//!
//! `(provider, model_id)` names a model on two unrelated write paths — the
//! admin catalogue (`/v1/admin/models`) and the tenant registry
//! (`/v1/tenants/me/models`) — and they disagreed. The catalogue bounded both;
//! the registry checked only that `model_id` was non-empty, so the same field
//! was 1–256 bytes through one door and effectively unbounded through the
//! other. One home, so the next door cannot invent a third answer.
//!
//! ## Why an unbounded `model_id` was not merely untidy
//!
//! Three separate failures, all fixed by the same bound:
//!
//!   1. **A tenant could make its own Models page permanently unreadable.**
//!      `schema/027` declares `model_id` TEXT. The btree behind
//!      `uq_tenant_model_entries_entry` caps an index entry at 2704 bytes
//!      *after compression*, so a single huge value is refused — but ~200 KB of
//!      repetitive text compresses small enough to index while still costing
//!      200 KB in the response. Three such rows push the registry page past its
//!      512 KiB ceiling, and the page is the only way to find the rows to
//!      delete them.
//!   2. **It stalled billing process-wide.** Every projected row consults the
//!      process-global rate cache (`model_rate_cache.rateAtRevision`), which
//!      hashes the whole `(provider, model_id)` pair under the cache's
//!      process-global lock that billing shares. 100 rows × 200 KB is ~20 MB
//!      of hashing per request, under a lock every tenant's charge computation
//!      waits on. This is the one that makes the bound urgent rather than
//!      tidy: the blast radius is other tenants, not just the one holding the
//!      oversized rows.
//!   3. **The write reported the wrong thing.** Past the index limit Postgres
//!      raised an index-size error, which the handler surfaced as
//!      `503 Database unavailable` — a client input fault reported as a server
//!      one, so nothing pointed at the actual problem.
//!
//! Bounding at the boundary makes all three unreachable, which is why the
//! ceiling guard downstream is a defect backstop rather than a live control.

/// Max bytes in a provider name. Same value the catalogue has always enforced.
pub const PROVIDER_MAX: usize = 64;

/// Max bytes in a `model_id`.
///
/// 256 is not a fresh judgement — it is the bound `/v1/admin/models` already
/// applied to this field, adopted here rather than re-derived so the two write
/// paths cannot drift again. It is ~5.7× the longest identifier any real
/// provider ships (`nvidia/NVIDIA-Nemotron-3-Ultra-550B-A55B-BF16`, 45 bytes),
/// so it bounds abuse without constraining legitimate names.
pub const MODEL_ID_MAX: usize = 256;

// ── tests ───────────────────────────────────────────────────────────────────

const std = @import("std");
const testing = std.testing;

test "the bounds are the ones the catalogue route already enforced" {
    // Pinned because the whole point is that one number governs both write
    // paths. Changing it here is visible; changing it in one handler is not.
    try testing.expectEqual(@as(usize, 64), PROVIDER_MAX);
    try testing.expectEqual(@as(usize, 256), MODEL_ID_MAX);
}

test "the bound clears real provider model ids with room to spare" {
    // The longest identifier in the seeded catalogue. A bound that a shipping
    // model name fails is an outage, so this is asserted rather than assumed.
    const longest_real = "nvidia/NVIDIA-Nemotron-3-Ultra-550B-A55B-BF16";
    try testing.expect(longest_real.len < MODEL_ID_MAX);
    try testing.expect(longest_real.len * 5 < MODEL_ID_MAX);
}

test "a bounded page cannot reach the registry body ceiling" {
    // The arithmetic that makes UZ-LIBRARY-005 a defect backstop rather than a
    // reachable outcome. Worst case: a full page where every row carries a
    // maximal model_id, plus generous room for the other projected fields.
    const MAX_PAGE_ROWS = 100;
    const OTHER_FIELDS_PER_ROW = 400; // id, secret_ref, provider, base_url, rates, keys
    const REGISTRY_BODY_CEILING = 512 * 1024;

    const worst_case = MAX_PAGE_ROWS * (MODEL_ID_MAX + OTHER_FIELDS_PER_ROW);
    try testing.expect(worst_case < REGISTRY_BODY_CEILING);
}
