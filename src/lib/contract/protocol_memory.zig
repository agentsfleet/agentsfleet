//! Durable fleet-memory wire sub-protocol — the hydrate/capture types + byte
//! budgets, extracted from protocol.zig (RULE FLL) and re-exported there so
//! `contract.protocol.MemoryDelta` (and siblings) stay unchanged for both build
//! graphs. Pure value types with no dependency back on protocol.zig — the
//! lease-embedding `RunnerChildInput` stays in protocol.zig to avoid a cycle.

/// Upper bound on the total memory bytes (sum of key+content+category over every
/// delta) one push may carry. The runner caps what it surfaces; the control plane
/// rejects beyond this. Oversized memory is truncated + logged, never silently
/// dropped whole. Single-sourced (RULE UFS) — both build graphs key off it.
pub const MAX_MEMORY_PUSH_BYTES: usize = 256 * 1024; // 256 KiB

/// Hard ceiling on the durable memory entries one fleet may accumulate across all
/// its runs. The per-push cap bounds a single push; this bounds the unbounded
/// growth a long-lived (or adversarial) fleet would otherwise build up — `enforceCap`
/// evicts beyond it server-side, tier-ordered: coldest non-core rows first, `core`
/// rows only when no non-core row remains. A backstop, not the primary bound
/// (stable-key overwrite + `memory_forget` are the fleet's own).
pub const MAX_MEMORY_ENTRIES_PER_AGENT: usize = 1000;

/// Byte budget for one hydration window. The `GET` Compactor is category-pinned:
/// every `core` entry that fits hydrates first (newest-first), then the newest
/// non-core entries fill the remainder of this budget; the dropped entries stay
/// durable in Postgres, unhydrated. Bounds the payload a run seeds into the child
/// regardless of how large the durable set has grown.
pub const HYDRATE_WINDOW_BYTES: usize = 256 * 1024; // 256 KiB

/// One durable fleet-memory item on the wire — the unit of both capture (POST
/// body) and hydrate (GET response). Carries no scope: the fleet is the
/// `{fleet_id}` path segment, server-validated against the runner's live lease.
/// One shape for a memory item, shared agentsfleetd <-> runner (RULE UFS).
pub const MemoryDelta = struct {
    key: []const u8,
    content: []const u8,
    category: []const u8,
};

/// POST /v1/runners/me/memory/{fleet_id} (Bearer runner_token) — capture the
/// run's memory for the path's fleet. `lease_id` + `fencing_token` ride the body
/// exactly like `ReportRequest`: the control plane loads that lease, verifies the
/// runner owns it, cross-checks `lease.fleet_id == {fleet_id}`, and fences the
/// write — a reclaimed holder (token below the fleet's live fencing seq) is
/// rejected UZ-RUN-005. The scope (`fleet_id`) is server-derived; each delta is
/// upserted (`ON CONFLICT (key, fleet_id) DO UPDATE`), so a retried push is idempotent.
pub const MemoryPushRequest = struct {
    lease_id: []const u8,
    fencing_token: u64,
    memory: []const MemoryDelta,
};

/// GET /v1/runners/me/memory/{fleet_id} reply — a compacted hydration window of
/// the path fleet's durable memory (category-pinned under `HYDRATE_WINDOW_BYTES`:
/// `core` entries first, then newest non-core; the dropped entries stay in
/// Postgres), which the runner parent seeds into the child's in-run store at run
/// start. The runner names the fleet it holds in its `LeasePayload`; the server
/// returns memory only for a fleet the runner holds a live lease for.
pub const MemoryHydrateResponse = struct {
    memory: []const MemoryDelta,
};
