//! Integration coverage for optimistic concurrency on the catalog row (M131 §9)
//! — the shared `http/etag.zig` capability's SECOND adopter.
//!
//! The claims, each true only if the If-Match verdict runs against the real row:
//!   - A stale `If-Match` is a 412 UZ-CATALOG-005 carrying the current etag, and
//!       nothing is written.
//!   - The destructive case: an operator's stale form re-sending `source_repo`
//!       after another operator moved the row is REFUSED — so the bundle
//!       (`content_hash`) is not discarded out from under it.
//!   - `If-Match` is opt-in: a PATCH without it still succeeds (last-write-wins),
//!       so existing callers are unbroken.
//!
//! Upload-kind onboards throughout — no network, no object storage.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TOKEN_PLATFORM = scope_fixtures.PLATFORM_ADMIN;
const CATALOG_URL = "/v1/admin/fleet-libraries";
const IF_MATCH = "if-match";

const Probe = struct {
    id: []const u8,
    repo: []const u8,
    moved_repo: []const u8,
};

const PROBE_STALE = Probe{
    .id = "etag-probe-stale",
    .repo = "agentsfleet/etag-probe-stale",
    .moved_repo = "agentsfleet/etag-probe-stale-moved",
};
const PROBE_RESEND = Probe{
    .id = "etag-probe-resend",
    .repo = "agentsfleet/etag-probe-resend",
    .moved_repo = "agentsfleet/etag-probe-resend-moved",
};
const PROBE_LOCK = Probe{
    .id = "etag-probe-lock",
    .repo = "agentsfleet/etag-probe-lock",
    .moved_repo = "agentsfleet/etag-probe-lock-moved",
};
const PROBE_OPTIONAL = Probe{
    .id = "etag-probe-optional",
    .repo = "agentsfleet/etag-probe-optional",
    .moved_repo = "agentsfleet/etag-probe-optional-moved",
};
const LOCK_POLL_NS = 20 * std.time.ns_per_ms;
const LOCK_POLL_LIMIT = 250;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn reset(conn: *pg.Conn, id: []const u8) !void {
    _ = try conn.exec("DELETE FROM core.fleet_library WHERE id = $1", .{id});
}

fn addProbe(h: *TestHarness, alloc: std.mem.Allocator, probe: Probe) !void {
    var skill_buf: [256]u8 = undefined;
    const skill = try std.fmt.bufPrint(
        &skill_buf,
        "---\nname: {s}\ndescription: Bundle-derived description.\nversion: 0.1.0\n---\nBody for the etag probe.\n",
        .{probe.id},
    );
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = probe.repo,
        .skill_markdown = skill,
        .replace = false,
    }, .{});
    defer alloc.free(body);
    const res = try (try (try h.post(CATALOG_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer res.deinit();
    try res.expectStatus(.created);
}

/// The catalog list's `etag` for the probe — the tag a row editor would send as
/// `If-Match`. Caller frees.
fn probeEtag(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8) ![]const u8 {
    const res = try (try h.get(CATALOG_URL).bearer(TOKEN_PLATFORM)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, res.body, .{});
    defer parsed.deinit();
    for (parsed.value.object.get("entries").?.array.items) |entry| {
        if (std.mem.eql(u8, entry.object.get("id").?.string, id)) {
            const tag = entry.object.get("etag") orelse return error.NoEtagOnEntry;
            return alloc.dupe(u8, tag.string);
        }
    }
    return error.ProbeMissing;
}

fn entryUrl(alloc: std.mem.Allocator, id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ CATALOG_URL, id });
}

fn hasBundle(conn: *pg.Conn, id: []const u8) !bool {
    var q = PgQuery.from(try conn.query("SELECT content_hash FROM core.fleet_library WHERE id = $1", .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return (try row.get(?[]const u8, 0)) != null;
}

fn readSourceRepo(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT source_repo FROM core.fleet_library WHERE id = $1", .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

const PatchOutcome = struct {
    status: u16 = 0,
};

const PatchWorker = struct {
    fn run(h: *TestHarness, url: []const u8, body: []const u8, if_match: []const u8, outcome: *PatchOutcome) void {
        const req = h.patch(url).bearer(TOKEN_PLATFORM) catch return;
        const with_body = req.json(body) catch return;
        const with_etag = with_body.header(IF_MATCH, if_match) catch return;
        const response = with_etag.send() catch return;
        defer response.deinit();
        outcome.status = response.status;
    }
};

fn waitForCatalogLockWaiter(conn: *pg.Conn) !void {
    var attempts: usize = 0;
    while (attempts < LOCK_POLL_LIMIT) : (attempts += 1) {
        var q = PgQuery.from(try conn.query(
            "SELECT count(*)::bigint FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND query LIKE '%core.fleet_library%'",
            .{},
        ));
        defer q.deinit();
        const row = try q.next() orelse return error.LockWaiterQueryEmpty;
        if (try row.get(i64, 0) > 0) return;
        @import("common").sleepNanos(LOCK_POLL_NS);
    }
    return error.CatalogPatchNeverBlocked;
}

test "integration: catalog stale If-Match → 412 with current etag, nothing written" {
    const alloc = std.testing.allocator;
    const probe = PROBE_STALE;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn, probe.id);
    defer reset(conn, probe.id) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    try addProbe(h, alloc, probe);

    const current = try probeEtag(h, alloc, probe.id);
    defer alloc.free(current);

    const url = try entryUrl(alloc, probe.id);
    defer alloc.free(url);
    const body = "{\"description\":\"edited by a racer\"}";

    // Stale tag → 412 UZ-CATALOG-005, body carries the current etag, row unchanged.
    const r_stale = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(body)).header(IF_MATCH, "\"deadbeef\"")).send();
    defer r_stale.deinit();
    try r_stale.expectStatus(.precondition_failed);
    try r_stale.expectErrorCode("UZ-CATALOG-005");
    const problem = try std.json.parseFromSlice(std.json.Value, alloc, r_stale.body, .{});
    defer problem.deinit();
    try std.testing.expectEqualStrings(current, problem.value.object.get("etag").?.string);

    // The matching tag → 200 (the reloaded save).
    const r_ok = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(body)).header(IF_MATCH, current)).send();
    defer r_ok.deinit();
    try r_ok.expectStatus(.ok);
}

test "integration: a stale re-send of source_repo cannot discard the bundle" {
    const alloc = std.testing.allocator;
    const probe = PROBE_RESEND;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn, probe.id);
    defer reset(conn, probe.id) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    try addProbe(h, alloc, probe);
    try std.testing.expect(try hasBundle(conn, probe.id)); // onboard stored a bundle

    // Operator A loads the row.
    const etag_a = try probeEtag(h, alloc, probe.id);
    defer alloc.free(etag_a);

    // Operator B renames the row (matching tag) — the row moves past etag_a.
    const url = try entryUrl(alloc, probe.id);
    defer alloc.free(url);
    const etag_b = try probeEtag(h, alloc, probe.id); // same version as A here
    defer alloc.free(etag_b);
    const r_b = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json("{\"description\":\"moved on by B\"}")).header(IF_MATCH, etag_b)).send();
    defer r_b.deinit();
    try r_b.expectStatus(.ok);

    // Operator A, still holding the stale tag, tries to repoint the source — the
    // destructive op: without If-Match this repoint would null content_hash and
    // draft the row. With the stale tag it is REFUSED (412), so the bundle survives.
    const moved_body = try std.json.Stringify.valueAlloc(alloc, .{ .source_repo = probe.moved_repo }, .{});
    defer alloc.free(moved_body);
    const r_a = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(moved_body)).header(IF_MATCH, etag_a)).send();
    defer r_a.deinit();
    try r_a.expectStatus(.precondition_failed);

    // The bundle was NOT discarded and the source was NOT repointed.
    try std.testing.expect(try hasBundle(conn, probe.id));
    const repo = try readSourceRepo(alloc, conn, probe.id);
    defer alloc.free(repo);
    try std.testing.expectEqualStrings(probe.repo, repo);
}

test "integration: If-Match check serializes with a concurrent catalog write" {
    const alloc = std.testing.allocator;
    const probe = PROBE_LOCK;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn, probe.id);
    defer reset(conn, probe.id) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    try addProbe(h, alloc, probe);

    const stale_etag = try probeEtag(h, alloc, probe.id);
    defer alloc.free(stale_etag);
    const url = try entryUrl(alloc, probe.id);
    defer alloc.free(url);
    const moved_body = try std.json.Stringify.valueAlloc(alloc, .{ .source_repo = probe.moved_repo }, .{});
    defer alloc.free(moved_body);

    _ = try conn.exec("BEGIN", .{});
    var transaction_open = true;
    var worker: ?std.Thread = null;
    defer {
        if (transaction_open) _ = conn.exec("ROLLBACK", .{}) catch {};
        if (worker) |thread| thread.join();
    }
    _ = try conn.exec(
        "UPDATE core.fleet_library SET description = 'committed by B' WHERE id = $1",
        .{probe.id},
    );

    var outcome: PatchOutcome = .{};
    worker = try std.Thread.spawn(.{}, PatchWorker.run, .{ h, url, moved_body, stale_etag, &outcome });
    try waitForCatalogLockWaiter(conn);
    _ = try conn.exec("COMMIT", .{});
    transaction_open = false;
    worker.?.join();
    worker = null;

    try std.testing.expectEqual(@as(u16, 412), outcome.status);
    const repo = try readSourceRepo(alloc, conn, probe.id);
    defer alloc.free(repo);
    try std.testing.expectEqualStrings(probe.repo, repo);
}

test "integration: catalog PATCH without If-Match still succeeds (opt-in)" {
    const alloc = std.testing.allocator;
    const probe = PROBE_OPTIONAL;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn, probe.id);
    defer reset(conn, probe.id) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    try addProbe(h, alloc, probe);

    const url = try entryUrl(alloc, probe.id);
    defer alloc.free(url);
    // No If-Match header at all → last-write-wins, unchanged from pre-M131.
    const r = try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json("{\"description\":\"blind save\"}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);
}
