const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("../db/test_fixtures.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");
const store = @import("crypto_store.zig");
const sql = @import("sql.zig");

const Scope = struct {
    tenant_id: []const u8,
    workspace_id: []const u8,
};

const ROUNDTRIP = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000002", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000002" };
const RELOCATE_KEY = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000003", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000003" };
const RELOCATE_WS = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000004", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000004" };
const RELOCATE_WS_TARGET = "0195b4ba-8d3a-7f13-8abc-cd0000000014";
const LEGACY = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000005", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000005" };
const MISSING = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000006", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000006" };
const UNSUPPORTED = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000007", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000007" };
const WRONG_VERSION = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000008", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000008" };
const MALFORMED = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000009", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000009" };
const PAYLOAD_FAILURE = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000010", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000010" };
const ALLOC_FAIL = Scope{ .tenant_id = "0195b4ba-8d3a-7f13-8abc-aa0000000011", .workspace_id = "0195b4ba-8d3a-7f13-8abc-cd0000000011" };
const VERSION_LEGACY: i32 = 1;
const VERSION_BOUND: i32 = 2;
const VERSION_UNSUPPORTED: i32 = 3;
const DELETE_ROWS = "DELETE FROM vault.secrets WHERE workspace_id = $1";
const SELECT_VERSION =
    "SELECT kek_version FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2";
const SELECT_CIPHERTEXT =
    "SELECT encrypted_dek, ciphertext FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2";
const SET_VERSION =
    "UPDATE vault.secrets SET kek_version = $3 WHERE workspace_id = $1 AND key_name = $2";
const BREAK_NONCE =
    "UPDATE vault.secrets SET dek_nonce = $3 WHERE workspace_id = $1 AND key_name = $2";
const BREAK_PAYLOAD_TAG =
    "UPDATE vault.secrets SET tag = $3 WHERE workspace_id = $1 AND key_name = $2";
const RELOCATE_ENVELOPE =
    \\UPDATE vault.secrets AS target
    \\   SET encrypted_dek = source.encrypted_dek,
    \\       dek_nonce = source.dek_nonce,
    \\       dek_tag = source.dek_tag,
    \\       nonce = source.nonce,
    \\       ciphertext = source.ciphertext,
    \\       tag = source.tag,
    \\       kek_version = source.kek_version
    \\  FROM vault.secrets AS source
    \\ WHERE target.workspace_id = $1 AND target.key_name = $2
    \\   AND source.workspace_id = $1 AND source.key_name = $3
;

fn seedWorkspace(conn: *pg.Conn, scope: Scope) !void {
    try base.seedTenantById(conn, scope.tenant_id, "vault-envelope-test");
    try base.seedWorkspaceWithTenant(conn, scope.workspace_id, scope.tenant_id);
    cp.setTestKek();
}

fn cleanup(conn: *pg.Conn, scope: Scope) void {
    _ = conn.exec(DELETE_ROWS, .{scope.workspace_id}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, scope.workspace_id);
    base.teardownTenantById(conn, scope.tenant_id);
}

fn readVersion(conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8) !i32 {
    var result = PgQuery.from(try conn.query(SELECT_VERSION, .{ workspace_id, key_name }));
    defer result.deinit();
    const row = (try result.next()).?;
    return row.get(i32, 0);
}

const CiphertextSnapshot = struct {
    wrapped_dek: []u8,
    ciphertext: []u8,

    fn deinit(self: CiphertextSnapshot, alloc: std.mem.Allocator) void {
        alloc.free(self.wrapped_dek);
        alloc.free(self.ciphertext);
    }
};

fn readCiphertext(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8) !CiphertextSnapshot {
    var result = PgQuery.from(try conn.query(SELECT_CIPHERTEXT, .{ workspace_id, key_name }));
    defer result.deinit();
    const row = (try result.next()).?;
    const wrapped_dek = try alloc.dupe(u8, try row.get([]u8, 0));
    errdefer alloc.free(wrapped_dek);
    return .{
        .wrapped_dek = wrapped_dek,
        .ciphertext = try alloc.dupe(u8, try row.get([]u8, 1)),
    };
}

fn seedLegacyEnvelope(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8, plaintext: []const u8) !void {
    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);
    var dek: [cp.KEY_LEN]u8 = undefined;
    defer std.crypto.secureZero(u8, &dek);
    try common.secureRandomBytes(&dek);

    const wrapped = try cp.encrypt(alloc, &dek, "", &kek);
    defer wrapped.deinit(alloc);
    const payload = try cp.encrypt(alloc, plaintext, "", &dek);
    defer payload.deinit(alloc);
    const secret_id = try id_format.generateVaultSecretId(alloc);
    defer alloc.free(secret_id);
    const now_ms = common.clock.nowMillis();
    _ = try conn.exec(sql.INSERT_SECRET, .{
        secret_id,    workspace_id,   key_name,           wrapped.ciphertext, &wrapped.nonce,
        &wrapped.tag, &payload.nonce, payload.ciphertext, &payload.tag,       VERSION_LEGACY,
        now_ms,
    });
}

test "integration: crypto store canonicalizes workspace id and upserts a fresh envelope" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, ROUNDTRIP);
    defer cleanup(handle.conn, ROUNDTRIP);

    const uppercase_workspace_id = try std.ascii.allocUpperString(alloc, ROUNDTRIP.workspace_id);
    defer alloc.free(uppercase_workspace_id);
    try store.store(alloc, handle.conn, uppercase_workspace_id, "roundtrip", "first");
    const first = try readCiphertext(alloc, handle.conn, ROUNDTRIP.workspace_id, "roundtrip");
    defer first.deinit(alloc);
    try std.testing.expectEqual(VERSION_BOUND, try readVersion(handle.conn, ROUNDTRIP.workspace_id, "roundtrip"));

    try store.store(alloc, handle.conn, ROUNDTRIP.workspace_id, "roundtrip", "second");
    const second = try readCiphertext(alloc, handle.conn, ROUNDTRIP.workspace_id, "roundtrip");
    defer second.deinit(alloc);
    const loaded = try store.load(alloc, handle.conn, ROUNDTRIP.workspace_id, "roundtrip");
    defer alloc.free(loaded);
    try std.testing.expectEqualStrings("second", loaded);
    try std.testing.expect(!std.mem.eql(u8, first.wrapped_dek, second.wrapped_dek));
    try std.testing.expect(!std.mem.eql(u8, first.ciphertext, second.ciphertext));
}

test "integration: crypto store rejects an envelope relocated to another key" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, RELOCATE_KEY);
    defer cleanup(handle.conn, RELOCATE_KEY);

    try store.store(alloc, handle.conn, RELOCATE_KEY.workspace_id, "victim", "victim-secret");
    try store.store(alloc, handle.conn, RELOCATE_KEY.workspace_id, "attacker", "attacker-secret");
    _ = try handle.conn.exec(RELOCATE_ENVELOPE, .{ RELOCATE_KEY.workspace_id, "attacker", "victim" });
    try std.testing.expectError(
        cp.SecretError.DecryptFailed,
        store.load(alloc, handle.conn, RELOCATE_KEY.workspace_id, "attacker"),
    );
}

test "integration: crypto store rejects an envelope relocated to another workspace" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, RELOCATE_WS);
    try base.seedWorkspaceWithTenant(handle.conn, RELOCATE_WS_TARGET, RELOCATE_WS.tenant_id);
    defer {
        _ = handle.conn.exec(DELETE_ROWS, .{RELOCATE_WS_TARGET}) catch {};
        base.teardownWorkspace(handle.conn, RELOCATE_WS_TARGET);
        cleanup(handle.conn, RELOCATE_WS);
    }

    try store.store(alloc, handle.conn, RELOCATE_WS.workspace_id, "shared", "victim-secret");
    try store.store(alloc, handle.conn, RELOCATE_WS_TARGET, "shared", "attacker-secret");
    const relocate_cross_workspace =
        \\UPDATE vault.secrets AS target
        \\   SET encrypted_dek = source.encrypted_dek, dek_nonce = source.dek_nonce,
        \\       dek_tag = source.dek_tag, nonce = source.nonce,
        \\       ciphertext = source.ciphertext, tag = source.tag
        \\  FROM vault.secrets AS source
        \\ WHERE target.workspace_id = $1 AND source.workspace_id = $2
        \\   AND target.key_name = $3 AND source.key_name = $3
    ;
    _ = try handle.conn.exec(relocate_cross_workspace, .{ RELOCATE_WS_TARGET, RELOCATE_WS.workspace_id, "shared" });
    try std.testing.expectError(cp.SecretError.DecryptFailed, store.load(alloc, handle.conn, RELOCATE_WS_TARGET, "shared"));
}

test "integration: crypto store reads a legacy envelope then rewrites version two" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, LEGACY);
    defer cleanup(handle.conn, LEGACY);

    try seedLegacyEnvelope(alloc, handle.conn, LEGACY.workspace_id, "legacy", "old-secret");
    const loaded = try store.load(alloc, handle.conn, LEGACY.workspace_id, "legacy");
    defer alloc.free(loaded);
    try std.testing.expectEqualStrings("old-secret", loaded);
    try std.testing.expectEqual(VERSION_LEGACY, try readVersion(handle.conn, LEGACY.workspace_id, "legacy"));

    try store.store(alloc, handle.conn, LEGACY.workspace_id, "legacy", "new-secret");
    try std.testing.expectEqual(VERSION_BOUND, try readVersion(handle.conn, LEGACY.workspace_id, "legacy"));
}

test "integration: crypto store returns not found for a missing key" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, MISSING);
    defer cleanup(handle.conn, MISSING);

    try std.testing.expectError(
        cp.SecretError.NotFound,
        store.load(alloc, handle.conn, MISSING.workspace_id, "missing"),
    );
}

test "integration: crypto store rejects an unsupported envelope version" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, UNSUPPORTED);
    defer cleanup(handle.conn, UNSUPPORTED);

    try store.store(alloc, handle.conn, UNSUPPORTED.workspace_id, "unsupported", "secret");
    _ = try handle.conn.exec(SET_VERSION, .{ UNSUPPORTED.workspace_id, "unsupported", VERSION_UNSUPPORTED });
    try std.testing.expectError(
        cp.SecretError.UnsupportedKekVersion,
        store.load(alloc, handle.conn, UNSUPPORTED.workspace_id, "unsupported"),
    );
}

test "integration: crypto store binds the envelope version" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, WRONG_VERSION);
    defer cleanup(handle.conn, WRONG_VERSION);

    try store.store(alloc, handle.conn, WRONG_VERSION.workspace_id, "wrong-version", "secret");
    _ = try handle.conn.exec(SET_VERSION, .{ WRONG_VERSION.workspace_id, "wrong-version", VERSION_LEGACY });
    try std.testing.expectError(cp.SecretError.DecryptFailed, store.load(alloc, handle.conn, WRONG_VERSION.workspace_id, "wrong-version"));
}

test "integration: crypto store rejects a malformed envelope" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, MALFORMED);
    defer cleanup(handle.conn, MALFORMED);

    try store.store(alloc, handle.conn, MALFORMED.workspace_id, "malformed", "secret");
    const short_nonce = [_]u8{ 1, 2, 3, 4 };
    _ = try handle.conn.exec(BREAK_NONCE, .{ MALFORMED.workspace_id, "malformed", &short_nonce });
    try std.testing.expectError(
        cp.SecretError.InvalidEnvelope,
        store.load(alloc, handle.conn, MALFORMED.workspace_id, "malformed"),
    );
}

test "integration: crypto store frees the unwrapped key after payload failure" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, PAYLOAD_FAILURE);
    defer cleanup(handle.conn, PAYLOAD_FAILURE);

    try store.store(alloc, handle.conn, PAYLOAD_FAILURE.workspace_id, "payload-failure", "secret");
    const wrong_tag = [_]u8{0} ** cp.TAG_LEN;
    _ = try handle.conn.exec(BREAK_PAYLOAD_TAG, .{ PAYLOAD_FAILURE.workspace_id, "payload-failure", &wrong_tag });
    try std.testing.expectError(cp.SecretError.DecryptFailed, store.load(alloc, handle.conn, PAYLOAD_FAILURE.workspace_id, "payload-failure"));
}

test "crypto store source keeps transient key zeroization" {
    const store_source = @embedFile("crypto_store.zig");
    const primitive_source = @embedFile("crypto_primitives.zig");
    // Relational, not a fixed count: EVERY unwrapped Key Encryption Key (KEK) is
    // zeroed. A pinned number says "there are two of these" and has to be edited
    // whenever a third caller appears — at which point it proves nothing about
    // the new one. Tying the zeroing count to the load count means a KEK loaded
    // without a matching `secureZero` fails here no matter how many exist.
    try std.testing.expectEqual(
        std.mem.count(u8, store_source, "cp.loadKek()"),
        std.mem.count(u8, store_source, "secureZero(u8, &kek)"),
    );
    try std.testing.expect(std.mem.count(u8, store_source, "secureZero(u8, &dek)") == 2);
    try std.testing.expect(std.mem.indexOf(u8, store_source, "secureZero(u8, dek_plain)") != null);
    try std.testing.expect(std.mem.indexOf(u8, primitive_source, "secureZero(u8, &key)") != null);
    try std.testing.expect(std.mem.indexOf(u8, primitive_source, "secureZero(u8, plaintext)") != null);
}

test "crypto store SQL includes envelope version" {
    const sql_source = @embedFile("sql.zig");
    try std.testing.expect(std.mem.indexOf(u8, sql_source, "kek_version") != null);
}

test "crypto store documents the random nonce invocation limit" {
    const primitive_source = @embedFile("crypto_primitives.zig");
    try std.testing.expect(std.mem.indexOf(u8, primitive_source, "KEK_WRAP_RANDOM_NONCE_INVOCATION_LIMIT_LOG2") != null);
    try std.testing.expect(std.mem.indexOf(u8, primitive_source, "collision probability is roughly 2^-33") != null);
}

// ── Allocation-failure sweeps over load/store ───────────────────────────────
// checkAllAllocationFailures fails each allocation site through the injected
// allocator in turn and asserts the call surfaces OutOfMemory with ZERO residue.
// The query itself runs on the conn's (pool) allocator, so the SELECT/INSERT
// always completes — only load/store's own dupe/AAD/encrypt/decrypt allocations
// fail, exercising the deferred free + secureZero unwind ladder. Key material
// never survives: kek/dek/dek_plain carry deferred secureZero that runs on
// every error return, and the drain (`result.deinit`) leaves the conn clean.

fn loadForFailCheck(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8) !void {
    const plaintext = try store.load(alloc, conn, workspace_id, key_name);
    alloc.free(plaintext);
}

fn storeForFailCheck(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8, plaintext: []const u8) !void {
    try store.store(alloc, conn, workspace_id, key_name, plaintext);
}

test "integration: crypto store load unwinds leak-free at every allocation-failure point" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, ALLOC_FAIL);
    defer cleanup(handle.conn, ALLOC_FAIL);

    // A real (version-2, AAD-bound) envelope to load — exercises buildAad on the
    // decrypt path, the widest allocation ladder.
    try store.store(alloc, handle.conn, ALLOC_FAIL.workspace_id, "afl-load", "top-secret-plaintext-material");
    try std.testing.checkAllAllocationFailures(alloc, loadForFailCheck, .{ handle.conn, ALLOC_FAIL.workspace_id, @as([]const u8, "afl-load") });
}

test "integration: crypto store store unwinds leak-free at every allocation-failure point" {
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspace(handle.conn, ALLOC_FAIL);
    defer cleanup(handle.conn, ALLOC_FAIL);

    // store upserts, so the single all-allocations-succeed run is idempotent;
    // every failing run returns OutOfMemory before the INSERT with kek/dek
    // zeroed and no leaked AAD / wrapped-DEK / ciphertext / secret-id buffer.
    try std.testing.checkAllAllocationFailures(alloc, storeForFailCheck, .{ handle.conn, ALLOC_FAIL.workspace_id, @as([]const u8, "afl-store"), @as([]const u8, "another-secret-value") });
}
