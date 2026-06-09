// HTTP integration tests for PATCH body fields (trigger_markdown +
// source_markdown). Single-thread cases — concurrent + deadlock harness
// lives in the sister file patch_concurrent_integration_test.zig.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. Tests that
// exercise the control-stream XADD additionally self-skip when Redis is
// unavailable (h.has_redis).
//
// Asserts the row-lock + field-merge transaction:
//   - trigger-only PATCH overlays the triggers half of config_json
//   - source-only PATCH overlays the tools/credentials/network/budget half
//   - both-fields PATCH lands in one SQL UPDATE (single updated_at bump)
//   - malformed reparse rolls back the txn cleanly (lock released)
//   - revision bump is visible to the §10a reloadZombieConfig path
//
// Uses the shared TestHarness — see docs/ZIG_RULES.md "HTTP Integration
// Tests — Use TestHarness".

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");

const MS_PER_SECOND = 1_000;
const EVAL_BRANCH_QUOTA = 100_000;

const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

// Canonical test tenant/workspace pair shared across handler integration
// tests so the verbatim TOKEN_USER from api_integration_test.zig validates.
// The original synthesized signature (against a non-canonical 0b6f01 pair)
// failed RS256 verification — handoff §10b risk note flagged regenerating
// via the test-token mint helper or copying the canonical token instead.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0b6f21";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
// User-role token bound to TEST_WORKSPACE_ID under TEST_TENANT_ID. Body-
// field PATCHes (no `status`) require workspace-member, so user role
// suffices. Verbatim copy of the canonical TOKEN_USER from
// `src/http/handlers/zombies/api_integration_test.zig` — same JWKS kid,
// same canonical 0a6f01/0a6f11 claims, validated RS256 signature.
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";

// Initial state — a webhook-only zombie. Tests will PATCH it to add a
// cron trigger, change the source body, or both.
const INITIAL_CONFIG_JSON =
    \\{"name":"patch-bot","x-usezombie":{"triggers":[{"type":"webhook","source":"github","events":["push"]}],"tools":["http_request"],"budget":{"daily_dollars":5.0}}}
;
const INITIAL_TRIGGER_MD =
    \\---
    \\name: patch-bot
    \\x-usezombie:
    \\  triggers:
    \\    - type: webhook
    \\      source: github
    \\      events: ["push"]
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
    \\# initial trigger
;
const INITIAL_SOURCE_MD =
    \\---
    \\name: patch-bot
    \\---
    \\# initial source body
;

const NEW_TRIGGER_MD_WITH_CRON =
    \\---
    \\name: patch-bot
    \\x-usezombie:
    \\  triggers:
    \\    - type: cron
    \\      schedule: "*/30 * * * *"
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
    \\# updated trigger — cron only
;

// parseSkillMetadata requires name+description+version in the frontmatter
// (config_markdown.zig:159-166). The PATCH body's source_markdown goes
// through that parser before the field-merge txn touches the row.
const NEW_SOURCE_MD =
    \\---
    \\name: patch-bot
    \\description: PATCH body-fields test bot
    \\version: 0.2.0
    \\---
    \\# new skill body — operator updated guidance
;

const MALFORMED_TRIGGER_MD =
    \\---
    \\name: patch-bot
    \\x-usezombie:
    \\  triggers: not-a-list   # YAML scalar where a list is required
    \\---
;

// Pre-encoded JSON-string forms of the multi-line bodies above. Evaluated
// at comptime so callers can build full request bodies via `++`. Without
// the explicit `comptime` here, the encoder fn's returned slice isn't
// resolvable as a comptime value at the call site.
const NEW_TRIGGER_JSON = jsonEscape(NEW_TRIGGER_MD_WITH_CRON);
const NEW_SOURCE_JSON = jsonEscape(NEW_SOURCE_MD);
const MALFORMED_TRIGGER_JSON = jsonEscape(MALFORMED_TRIGGER_MD);

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    _ = h.tryConnectRedis();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixture(conn);
    return h;
}

fn seedFixture(conn: *pg.Conn) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'PatchBodyFieldsTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'patch-bot', $3, $4, $5::jsonb, 'active', $6, $6)
        \\ON CONFLICT (id) DO UPDATE SET
        \\    source_markdown  = EXCLUDED.source_markdown,
        \\    trigger_markdown = EXCLUDED.trigger_markdown,
        \\    config_json      = EXCLUDED.config_json,
        \\    status           = 'active',
        \\    updated_at       = EXCLUDED.updated_at
    , .{ ZOMBIE_ID, TEST_WORKSPACE_ID, INITIAL_SOURCE_MD, INITIAL_TRIGGER_MD, INITIAL_CONFIG_JSON, now });
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{ZOMBIE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn patchUrl() ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}", .{ TEST_WORKSPACE_ID, ZOMBIE_ID });
}

fn readRow(conn: *pg.Conn) !struct {
    name: []const u8,
    config_json: []const u8,
    trigger_markdown: ?[]const u8,
    source_markdown: []const u8,
    updated_at: i64,
} {
    var q = PgQuery.from(try conn.query(
        \\SELECT name, config_json::text, trigger_markdown, source_markdown, updated_at
        \\FROM core.zombies WHERE id = $1::uuid
    , .{ZOMBIE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    return .{
        .name = try ALLOC.dupe(u8, try row.get([]const u8, 0)),
        .config_json = try ALLOC.dupe(u8, try row.get([]const u8, 1)),
        .trigger_markdown = if (try row.get(?[]const u8, 2)) |v| try ALLOC.dupe(u8, v) else null,
        .source_markdown = try ALLOC.dupe(u8, try row.get([]const u8, 3)),
        .updated_at = try row.get(i64, 4),
    };
}

fn freeRow(r: anytype) void {
    ALLOC.free(r.name);
    ALLOC.free(r.config_json);
    if (r.trigger_markdown) |t| ALLOC.free(t);
    ALLOC.free(r.source_markdown);
}

// ── §1 — trigger_markdown-only PATCH ──────────────────────────────────────

test "integration: PATCH trigger_markdown only — reparses, persists, bumps revision" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try patchUrl();
    defer ALLOC.free(url);

    const before = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(before);

    // Wait one ms so updated_at strictly advances even on fast hosts.
    @import("common").sleepNanos(2 * std.time.ns_per_ms);

    const body = "{\"trigger_markdown\":" ++ NEW_TRIGGER_JSON ++ "}";
    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"config_revision\":"));

    const after = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(after);

    try std.testing.expectEqualStrings("patch-bot", after.name);
    try std.testing.expect(after.updated_at > before.updated_at);
    // trigger half changed — cron schedule now present, github webhook gone.
    try std.testing.expect(std.mem.indexOf(u8, after.config_json, "\"schedule\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, after.config_json, "\"*/30 * * * *\"") != null);
    // trigger_markdown column reflects the new body.
    try std.testing.expect(after.trigger_markdown != null);
    try std.testing.expect(std.mem.indexOf(u8, after.trigger_markdown.?, "type: cron") != null);
    // source half is untouched (still the initial source body).
    try std.testing.expectEqualStrings(INITIAL_SOURCE_MD, after.source_markdown);

    const c = try h.acquireConn();
    defer h.releaseConn(c);
    cleanup(c);
}

// ── §2 — source_markdown-only PATCH ───────────────────────────────────────

test "integration: PATCH source_markdown only — overlays source body, leaves triggers alone" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try patchUrl();
    defer ALLOC.free(url);

    @import("common").sleepNanos(2 * std.time.ns_per_ms);

    const body = "{\"source_markdown\":" ++ NEW_SOURCE_JSON ++ "}";
    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const after = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(after);

    try std.testing.expectEqualStrings(NEW_SOURCE_MD, after.source_markdown);
    // Triggers half intact: original webhook still present, no cron schedule.
    try std.testing.expect(std.mem.indexOf(u8, after.config_json, "\"webhook\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, after.config_json, "\"schedule\"") == null);

    const c = try h.acquireConn();
    defer h.releaseConn(c);
    cleanup(c);
}

test "integration: PATCH rejects an oversized source_markdown (same 64KiB cap as create)" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try patchUrl();
    defer ALLOC.free(url);

    // 64KiB + 1 of JSON-safe filler — one byte over the create-time cap. The body
    // now rides every lease, so PATCH must reject it like create does.
    const filler = try ALLOC.alloc(u8, 64 * 1024 + 1);
    defer ALLOC.free(filler);
    @memset(filler, 'a');
    const body = try std.fmt.allocPrint(ALLOC, "{{\"source_markdown\":\"{s}\"}}", .{filler});
    defer ALLOC.free(body);

    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
}

// ── §3 — both fields land in one SQL UPDATE ───────────────────────────────

test "integration: PATCH trigger_markdown + source_markdown — single UPDATE, both halves" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try patchUrl();
    defer ALLOC.free(url);

    const before = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(before);

    @import("common").sleepNanos(2 * std.time.ns_per_ms);

    const body = "{\"trigger_markdown\":" ++ NEW_TRIGGER_JSON ++
        ",\"source_markdown\":" ++ NEW_SOURCE_JSON ++ "}";
    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const after = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(after);

    // One revision bump (not two — both halves rode the same UPDATE).
    try std.testing.expect(after.updated_at > before.updated_at);
    // Triggers half: cron present.
    try std.testing.expect(std.mem.indexOf(u8, after.config_json, "\"*/30 * * * *\"") != null);
    // Source half: new body persisted.
    try std.testing.expectEqualStrings(NEW_SOURCE_MD, after.source_markdown);

    const c = try h.acquireConn();
    defer h.releaseConn(c);
    cleanup(c);
}

// ── §4 — malformed reparse → 400 + lock released ──────────────────────────

test "integration: PATCH malformed trigger_markdown — 400, next PATCH on same row succeeds" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try patchUrl();
    defer ALLOC.free(url);

    const before = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(before);

    const bad_body = "{\"trigger_markdown\":" ++ MALFORMED_TRIGGER_JSON ++ "}";
    const r_bad = try (try (try h.request(.PATCH, url).bearer(TOKEN_USER)).json(bad_body)).send();
    defer r_bad.deinit();
    try r_bad.expectStatus(.bad_request);

    // DB row unchanged — txn rolled back.
    const mid = blk: {
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        break :blk try readRow(c);
    };
    defer freeRow(mid);
    try std.testing.expectEqual(before.updated_at, mid.updated_at);

    // Lock released — a follow-up PATCH on the same row succeeds immediately
    // (no leftover row-lock from the rolled-back txn). 5-second lock_timeout
    // means if the lock leaked, this would hang ~5s — but with rollback it
    // returns in milliseconds. Asserting status alone is sufficient: a hang
    // would manifest as test timeout failure, not a wrong status code.
    const t0 = clock.nowMillis();
    const good_body = "{\"trigger_markdown\":" ++ NEW_TRIGGER_JSON ++ "}";
    const r_good = try (try (try h.request(.PATCH, url).bearer(TOKEN_USER)).json(good_body)).send();
    defer r_good.deinit();
    try r_good.expectStatus(.ok);
    const elapsed_ms = clock.nowMillis() - t0;
    try std.testing.expect(elapsed_ms < MS_PER_SECOND);

    const c = try h.acquireConn();
    defer h.releaseConn(c);
    cleanup(c);
}

// The config-reload seam test that lived here drove the worker-only
// `event_loop.reloadZombieConfig`, deleted at the M80 cutover. Config is now
// resolved fresh from Postgres on every lease, so PATCH writes the row and the
// next lease picks it up — there is no reload signal to assert. The vestigial
// `zombie:control` publish was removed with the dead `control_stream` module.

// Comptime JSON-string-encode a multi-line literal. `comptime`-block
// concatenation needs `return` outside the block so the caller sees the
// concatenated slice as a comptime value, not a runtime fn result.
fn jsonEscape(comptime s: []const u8) []const u8 {
    @setEvalBranchQuota(EVAL_BRANCH_QUOTA);
    comptime var out: []const u8 = "\"";
    inline for (s) |c| {
        out = out ++ switch (c) {
            '"' => "\\\"",
            '\\' => "\\\\",
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            else => &[_]u8{c},
        };
    }
    return out ++ "\"";
}
