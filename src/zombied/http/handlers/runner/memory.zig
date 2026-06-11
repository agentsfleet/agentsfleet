//! Runner-plane durable-memory endpoints — the hydrate/capture loop.
//!
//!   GET  /v1/runners/me/memory/{zombie_id}   → innerRunnerMemoryHydrate
//!        the runner parent seeds the child's in-run store from this at run start;
//!        the reply is a recency+byte-budget window (cold tail stays in Postgres).
//!   POST /v1/runners/me/memory/{zombie_id}   → innerRunnerMemoryCapture
//!        MemoryPushRequest { lease_id, fencing_token, memory: []MemoryDelta }.
//!
//! The runner NAMES the zombie (`{zombie_id}`) — it already holds it in its
//! LeasePayload, so explicit naming beats inferring it from ambient lease state.
//! Auth: `runnerBearer` (`zrn_`), never the tenant plane. GET authorizes by "the
//! runner holds a live lease for {zombie_id}"; POST loads the body's `lease_id`
//! (like `/reports`), cross-checks `lease.zombie_id == {zombie_id}` (IDOR guard),
//! and fences the write — a reclaimed holder (token below the zombie's live fencing
//! seq) is rejected UZ-RUN-005 and writes nothing. Every query scopes
//! `WHERE zombie_id = $1` at the database (never a fetch-all + in-memory filter).

const std = @import("std");
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
const adapter = @import("../../../memory/zombie_memory.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_memory);

const S_RUNNER_IDENTITY_REQUIRED = "runner identity required";
const S_ROLE_SWITCH_FAILED = "memory backend role switch failed";
const S_MALFORMED_ZOMBIE_ID = "zombie_id must be a valid UUIDv7";
const S_MALFORMED_LEASE_ID = "lease_id must be a valid UUIDv7";
const S_NO_LIVE_LEASE = "runner holds no live lease for this zombie";

// ── POST /v1/runners/me/memory/{zombie_id} — capture ───────────────────────

/// Persist the run's memory under the path's zombie. Fencing-verified; the
/// `zombie_id` is validated against the runner's live lease. Each delta is
/// upserted (idempotent). Memory content is NEVER logged — only the count + scope.
pub fn innerRunnerMemoryCapture(hx: Hx, req: *httpz.Request, zombie_id: []const u8) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, S_RUNNER_IDENTITY_REQUIRED);
        return;
    };
    if (!id_format.isUuidV7(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_ZOMBIE_ID);
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
    // AND it is for this path zombie (IDOR cross-check), active + unexpired. The
    // zombie's live fencing seq fences the write — a reclaimed holder is below it.
    const live_seq = (pushLeaseSeq(conn, runner_id, body.lease_id, zombie_id, clock.nowMillis()) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, S_NO_LIVE_LEASE);
        return;
    };
    if (body.fencing_token < live_seq) {
        log.info("memory_push_fenced", .{ .zombie_id = zombie_id, .fencing_token = body.fencing_token, .live_seq = live_seq });
        hx.fail(ec.ERR_RUN_STALE_FENCING_TOKEN, "Lease superseded by a newer holder; memory push rejected");
        return;
    }

    if (!h.setMemoryRole(conn)) {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, S_ROLE_SWITCH_FAILED);
        return;
    }
    defer h.resetRole(conn);

    var stored: usize = 0;
    var skipped: usize = 0;
    var bytes: usize = 0;
    for (body.memory) |d| {
        if (d.key.len == 0 or d.key.len > h.MAX_KEY_LEN or
            d.content.len == 0 or d.content.len > h.MAX_CONTENT_LEN or
            d.category.len == 0 or d.category.len > h.MAX_CATEGORY_LEN)
        {
            skipped += 1;
            metrics_memory.incCaptureSkipped();
            continue;
        }
        bytes += adapter.entryBytes(d);
        if (bytes > protocol.MAX_MEMORY_PUSH_BYTES) {
            // Truncate, don't drop the whole push (Failure Modes: oversized deltas).
            metrics_memory.incCaptureTruncated();
            log.warn("memory_push_truncated", .{ .zombie_id = zombie_id, .stored = stored, .cap = protocol.MAX_MEMORY_PUSH_BYTES });
            break;
        }
        const id = h.genId(hx.alloc);
        const ts = h.nowTs(hx.alloc);
        adapter.storeEntry(conn, id, zombie_id, d.key, d.content, d.category, ts) catch {
            metrics_memory.incMemoryPushFailure();
            log.warn("memory_store_failed", .{ .error_code = ec.ERR_MEM_UNAVAILABLE, .zombie_id = zombie_id });
            hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory store failed");
            return;
        };
        stored += 1;
    }

    // Backstop the durable set after the push: evict the coldest beyond the cap. A
    // cap-eviction blip must not fail a capture that already persisted (and counts
    // nothing — the eviction counter moves only on a reported eviction).
    const evicted = adapter.enforceCap(conn, zombie_id, protocol.MAX_MEMORY_ENTRIES_PER_ZOMBIE) catch blk: {
        log.warn("memory_cap_evict_failed", .{ .zombie_id = zombie_id });
        break :blk 0;
    };
    metrics_memory.incCapEvictions(evicted);

    metrics_memory.incMemoryCaptured(stored);
    log.info("memory_captured", .{ .zombie_id = zombie_id, .stored = stored, .skipped = skipped, .evicted = evicted });
    hx.ok(.ok, .{ .stored = stored, .skipped = skipped, .request_id = hx.req_id });
}

// ── GET /v1/runners/me/memory/{zombie_id} — hydrate ────────────────────────

/// Return a recency + byte-budget window of the path zombie's memory, scoped
/// `WHERE zombie_id = $1` at the database and passed through the `.recency_window`
/// `Compactor` (the cold tail stays in Postgres). The runner must hold a live lease.
pub fn innerRunnerMemoryHydrate(hx: Hx, zombie_id: []const u8) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, S_RUNNER_IDENTITY_REQUIRED);
        return;
    };
    if (!id_format.isUuidV7(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_ZOMBIE_ID);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = (liveLeaseSeq(conn, runner_id, zombie_id, clock.nowMillis()) catch {
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

    const rows = adapter.listAll(hx.alloc, conn, zombie_id) catch {
        hx.fail(ec.ERR_MEM_UNAVAILABLE, "memory hydrate failed");
        return;
    };
    // Compact to a recency + byte-budget window; the cold tail stays in Postgres.
    const compactor: adapter.Compactor = .{ .recency_window = protocol.HYDRATE_WINDOW_BYTES };
    const entries = compactor.compact(rows);
    metrics_memory.setMemoryHydrationEntries(entries.len);
    // The window's loss is the difference between the full set and the kept
    // prefix — entryBytes is the same formula the Compactor budgets on.
    var dropped_bytes: usize = 0;
    for (rows[entries.len..]) |d| dropped_bytes += adapter.entryBytes(d);
    metrics_memory.incHydrationDropped(rows.len - entries.len, dropped_bytes);
    log.info("memory_hydrated", .{ .zombie_id = zombie_id, .count = entries.len, .dropped = rows.len - entries.len, .dropped_bytes = dropped_bytes });
    hx.ok(.ok, protocol.MemoryHydrateResponse{ .memory = entries });
}

// ── lease authorization ────────────────────────────────────────────────────

/// The zombie's live fencing seq IFF the presenting runner holds a live (active,
/// unexpired) lease for it — `COALESCE(affinity.fencing_seq, lease.fencing_token)`
/// so a reclaim that bumped the seq strands the old holder below it. Null when the
/// runner holds no live lease for the zombie; error on DB failure.
pub fn liveLeaseSeq(conn: *pg.Conn, runner_id: []const u8, zombie_id: []const u8, now_ms: i64) !?u64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
        \\FROM fleet.runner_leases l
        \\LEFT JOIN fleet.runner_affinity a ON a.zombie_id = l.zombie_id
        \\WHERE l.runner_id = $1::uuid AND l.zombie_id = $2::uuid
        \\  AND l.status = $3 AND l.lease_expires_at > $4
        \\ORDER BY l.created_at DESC
        \\LIMIT 1
    , .{ runner_id, zombie_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    // fencing seqs are server-issued and monotonic (never negative); a negative
    // value is corrupt/tampered data — fail it cleanly instead of @intCast trapping.
    const raw = try row.get(i64, 0);
    if (raw < 0) return error.InvalidFencingSeq;
    return @intCast(raw);
}

/// The zombie's live fencing seq IFF the presenting runner holds the named LEASE for
/// the named zombie, active + unexpired. Keyed by `lease_id` (like `/reports`) AND
/// `zombie_id`, so a lease that exists but is for another zombie yields null — the
/// IDOR cross-check is the `WHERE` itself. `COALESCE(affinity.fencing_seq,
/// lease.fencing_token)` so a reclaim that bumped the seq strands the old holder
/// below it. Null when no such live lease; error on DB failure.
pub fn pushLeaseSeq(conn: *pg.Conn, runner_id: []const u8, lease_id: []const u8, zombie_id: []const u8, now_ms: i64) !?u64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
        \\FROM fleet.runner_leases l
        \\LEFT JOIN fleet.runner_affinity a ON a.zombie_id = l.zombie_id
        \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.zombie_id = $3::uuid
        \\  AND l.status = $4 AND l.lease_expires_at > $5
        \\LIMIT 1
    , .{ lease_id, runner_id, zombie_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    // fencing seqs are server-issued and monotonic (never negative); a negative
    // value is corrupt/tampered data — fail it cleanly instead of @intCast trapping.
    const raw = try row.get(i64, 0);
    if (raw < 0) return error.InvalidFencingSeq;
    return @intCast(raw);
}
