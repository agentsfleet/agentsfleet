//! Runner-plane durable-memory endpoints — the hydrate/capture loop.
//!
//!   GET  /v1/runners/me/memory/{fleet_id}   → innerRunnerMemoryHydrate
//!        the runner parent seeds the child's in-run store from this at run start;
//!        the reply is a category-pinned byte-budget window — `core` entries
//!        first, then newest non-core (cold tail stays in Postgres).
//!   POST /v1/runners/me/memory/{fleet_id}   → innerRunnerMemoryCapture
//!        MemoryPushRequest { lease_id, fencing_token, memory: []MemoryDelta }.
//!
//! The runner NAMES the fleet (`{fleet_id}`) — it already holds it in its
//! LeasePayload, so explicit naming beats inferring it from ambient lease state.
//! Auth: `runnerBearer` (`agt_r`), never the tenant plane. GET authorizes by "the
//! runner holds a live lease for {fleet_id}"; POST loads the body's `lease_id`
//! (like `/reports`), cross-checks `lease.fleet_id == {fleet_id}` (IDOR guard),
//! and fences the write — a reclaimed holder (token below the fleet's live fencing
//! seq) is rejected UZ-RUN-005 and writes nothing. Every query scopes
//! `WHERE fleet_id = $1` at the database (never a fetch-all + in-memory filter).

const std = @import("std");
const sql = @import("sql.zig");
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");
const clock = @import("common").clock;
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const protocol = @import("contract").protocol;
const hx_mod = @import("../hx.zig");
const h = @import("../memory/helpers.zig");
const adapter = @import("../../../memory/fleet_memory.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_memory);

const S_RUNNER_IDENTITY_REQUIRED = "runner identity required";
const S_ROLE_SWITCH_FAILED = "memory backend role switch failed";
const S_MALFORMED_FLEET_ID = "fleet_id must be a valid UUIDv7";
const S_MALFORMED_LEASE_ID = "lease_id must be a valid UUIDv7";
const S_NO_LIVE_LEASE = "runner holds no live lease for this fleet";

// ── POST /v1/runners/me/memory/{fleet_id} — capture ───────────────────────

/// Persist the run's memory under the path's fleet. Fencing-verified; the
/// `fleet_id` is validated against the runner's live lease. Each delta is
/// upserted (idempotent). Memory content is NEVER logged — only the count + scope.
pub fn innerRunnerMemoryCapture(hx: Hx, req: *httpz.Request, fleet_id: []const u8) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, S_RUNNER_IDENTITY_REQUIRED);
        return;
    };
    if (!id_format.isUuidV7(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_FLEET_ID);
        return;
    }
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(protocol.MemoryPushRequest, hx.alloc, raw_body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;
    if (!id_format.isUuidV7(body.lease_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_LEASE_ID);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Authorize like /reports: load the body's lease_id, require the runner owns it
    // AND it is for this path fleet (IDOR cross-check), active + unexpired. The
    // fleet's live fencing seq fences the write — a reclaimed holder is below it.
    // One clock read per push: the lease check, every entry timestamp, and the
    // sweep cutoff all derive from it (worst-case skew: a row lives one push longer).
    const now_ms = clock.nowMillis();
    const live_seq = (pushLeaseSeq(conn, runner_id, body.lease_id, fleet_id, now_ms) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, S_NO_LIVE_LEASE);
        return;
    };
    if (body.fencing_token < live_seq) {
        log.debug("memory_push_fenced", .{ .fleet_id = fleet_id, .fencing_token = body.fencing_token, .live_seq = live_seq });
        hx.fail(ec.ERR_RUN_STALE_FENCING_TOKEN, "Lease superseded by a newer holder; memory push rejected");
        return;
    }

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, S_ROLE_SWITCH_FAILED);
        return;
    }
    defer h.resetRole(conn);

    const counts = storeDeltas(hx, conn, fleet_id, body.memory, now_ms) orelse return;

    // Expire aged scratch BEFORE cap enforcement, same role window: an
    // already-expired `daily` row must not occupy a cap slot during victim
    // selection, or eviction deletes a durable row in the doomed row's place.
    // A sweep blip must not fail a capture that already persisted.
    const swept = adapter.sweepExpiredDaily(conn, fleet_id, now_ms - adapter.DAILY_RETENTION_MS) catch blk: {
        log.warn("memory_daily_sweep_failed", .{ .error_code = ec.ERR_MEM_UNAVAILABLE, .fleet_id = fleet_id });
        break :blk 0;
    };

    // Backstop the durable set after the push: evict the coldest beyond the cap. A
    // cap-eviction blip must not fail a capture that already persisted (and counts
    // nothing — the eviction counter moves only on a reported eviction).
    const evicted = adapter.enforceCap(conn, fleet_id, protocol.MAX_MEMORY_ENTRIES_PER_AGENT) catch blk: {
        log.warn("memory_cap_evict_failed", .{ .error_code = ec.ERR_MEM_UNAVAILABLE, .fleet_id = fleet_id });
        break :blk 0;
    };
    metrics_memory.incCapEvictions(evicted);

    metrics_memory.incMemoryCaptured(counts.stored);
    log.debug("memory_captured", .{ .fleet_id = fleet_id, .stored = counts.stored, .skipped = counts.skipped, .evicted = evicted, .swept = swept });
    hx.ok(.ok, .{ .stored = counts.stored, .skipped = counts.skipped, .request_id = hx.req_id });
}

const StoreCounts = struct { stored: usize, skipped: usize };

/// Validate and upsert the push's deltas at `ts_ms`, byte-capped. Returns the
/// stored/skipped tallies, or null after `hx.fail` when a store error already
/// answered the request (the caller just returns).
fn storeDeltas(hx: Hx, conn: *pg.Conn, fleet_id: []const u8, deltas: []const protocol.MemoryDelta, ts_ms: i64) ?StoreCounts {
    var counts: StoreCounts = .{ .stored = 0, .skipped = 0 };
    var bytes: usize = 0;
    for (deltas) |d| {
        if (d.key.len == 0 or d.key.len > h.MAX_KEY_LEN or
            d.content.len == 0 or d.content.len > h.MAX_CONTENT_LEN or
            d.category.len == 0 or d.category.len > h.MAX_CATEGORY_LEN)
        {
            counts.skipped += 1;
            metrics_memory.incCaptureSkipped();
            continue;
        }
        bytes += adapter.entryBytes(d);
        if (bytes > protocol.MAX_MEMORY_PUSH_BYTES) {
            // Truncate, don't drop the whole push (Failure Modes: oversized deltas).
            metrics_memory.incCaptureTruncated();
            log.warn("memory_push_truncated", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .stored = counts.stored, .cap = protocol.MAX_MEMORY_PUSH_BYTES });
            break;
        }
        const id = h.genId(hx.alloc);
        adapter.storeEntry(conn, id, fleet_id, d.key, d.content, d.category, ts_ms) catch {
            metrics_memory.incMemoryPushFailure();
            log.warn("memory_store_failed", .{ .error_code = ec.ERR_MEM_UNAVAILABLE, .fleet_id = fleet_id });
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory store failed");
            return null;
        };
        counts.stored += 1;
    }
    return counts;
}

// ── GET /v1/runners/me/memory/{fleet_id} — hydrate ────────────────────────

/// Return a category-pinned byte-budget window of the path fleet's memory —
/// every fitting `core` entry, then the newest non-core entries — scoped
/// `WHERE fleet_id = $1` at the database and passed through the `.selective`
/// `Compactor` (the cold tail stays in Postgres). The runner must hold a live lease.
pub fn innerRunnerMemoryHydrate(hx: Hx, fleet_id: []const u8) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, S_RUNNER_IDENTITY_REQUIRED);
        return;
    };
    if (!id_format.isUuidV7(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_FLEET_ID);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = (liveLeaseSeq(conn, runner_id, fleet_id, clock.nowMillis()) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, S_NO_LIVE_LEASE);
        return;
    };

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, S_ROLE_SWITCH_FAILED);
        return;
    }
    defer h.resetRole(conn);

    const rows = adapter.listAll(hx.alloc, conn, fleet_id) catch {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory hydrate failed");
        return;
    };
    // Compact to a category-pinned byte window (`core` first, then the newest
    // non-core entries); the dropped entries stay in Postgres.
    const compactor: adapter.Compactor = .{ .selective = protocol.HYDRATE_WINDOW_BYTES };
    const entries = compactor.compact(rows);
    metrics_memory.setMemoryHydrationEntries(entries.len);
    // compact() leaves the slice a permutation of its input — the kept entries
    // occupy the head, so the tail is exactly the dropped set. entryBytes is
    // the same formula the Compactor budgets on.
    const dropped_bytes = adapter.sumBytes(rows[entries.len..]);
    metrics_memory.incHydrationDropped(rows.len - entries.len, dropped_bytes);
    log.debug("memory_hydrated", .{ .fleet_id = fleet_id, .count = entries.len, .dropped = rows.len - entries.len, .dropped_bytes = dropped_bytes });
    hx.ok(.ok, protocol.MemoryHydrateResponse{ .memory = entries });
}

// ── lease authorization ────────────────────────────────────────────────────

/// The fleet's live fencing seq IFF the presenting runner holds a live (active,
/// unexpired) lease for it — `COALESCE(affinity.fencing_seq, lease.fencing_token)`
/// so a reclaim that bumped the seq strands the old holder below it. Null when the
/// runner holds no live lease for the fleet; error on DB failure.
pub fn liveLeaseSeq(conn: *pg.Conn, runner_id: []const u8, fleet_id: []const u8, now_ms: i64) !?u64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_LIVE_FENCE_BY_FLEET, .{ runner_id, fleet_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    // fencing seqs are server-issued and monotonic (never negative); a negative
    // value is corrupt/tampered data — fail it cleanly instead of @intCast trapping.
    const raw = try row.get(i64, 0);
    if (raw < 0) return error.InvalidFencingSeq;
    return @intCast(raw);
}

/// The fleet's live fencing seq IFF the presenting runner holds the named LEASE for
/// the named fleet, active + unexpired. Keyed by `lease_id` (like `/reports`) AND
/// `fleet_id`, so a lease that exists but is for another fleet yields null — the
/// IDOR cross-check is the `WHERE` itself. `COALESCE(affinity.fencing_seq,
/// lease.fencing_token)` so a reclaim that bumped the seq strands the old holder
/// below it. Null when no such live lease; error on DB failure.
pub fn pushLeaseSeq(conn: *pg.Conn, runner_id: []const u8, lease_id: []const u8, fleet_id: []const u8, now_ms: i64) !?u64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_LIVE_FENCE_BY_LEASE, .{ lease_id, runner_id, fleet_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    // fencing seqs are server-issued and monotonic (never negative); a negative
    // value is corrupt/tampered data — fail it cleanly instead of @intCast trapping.
    const raw = try row.get(i64, 0);
    if (raw < 0) return error.InvalidFencingSeq;
    return @intCast(raw);
}
