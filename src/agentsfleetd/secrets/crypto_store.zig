//! Database-backed secret storage using envelope encryption.
//! Depends on crypto_primitives for all crypto operations.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");
const sql = @import("sql.zig");
const secure_memory = @import("secure_memory.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");

const log = logging.scoped(.secrets);

const KEY_LEN = cp.KEY_LEN;
const NONCE_LEN = cp.NONCE_LEN;
const TAG_LEN = cp.TAG_LEN;
const KEK_VERSION_LEGACY: i32 = 1;
const KEK_VERSION_AAD_BOUND: i32 = 2;
const AAD_SEPARATOR: u8 = 0x1f;
const AAD_FORMAT = "{s}{c}{s}{c}{d}";

fn buildAad(alloc: std.mem.Allocator, workspace_id: []const u8, key_name: []const u8, kek_version: i32) ![]u8 {
    const canonical_workspace_id = try std.ascii.allocLowerString(alloc, workspace_id);
    defer alloc.free(canonical_workspace_id);
    return std.fmt.allocPrint(alloc, AAD_FORMAT, .{ canonical_workspace_id, AAD_SEPARATOR, key_name, AAD_SEPARATOR, kek_version });
}

/// Store encrypted secret in vault.secrets with envelope encryption.
pub fn store(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);

    var dek: [KEY_LEN]u8 = undefined;
    defer std.crypto.secureZero(u8, &dek);
    try constants.secureRandomBytes(&dek);

    const aad = try buildAad(alloc, workspace_id, key_name, KEK_VERSION_AAD_BOUND);
    defer alloc.free(aad);

    const wrapped_dek = try cp.encrypt(alloc, dek[0..], aad, &kek);
    defer wrapped_dek.deinit(alloc);

    const encrypted_payload = try cp.encrypt(alloc, plaintext, aad, &dek);
    defer encrypted_payload.deinit(alloc);

    const now_ms = clock.nowMillis();

    const secret_id = try id_format.generateVaultSecretId(alloc);
    defer alloc.free(secret_id);
    _ = try conn.exec(sql.INSERT_SECRET, .{
        secret_id,
        workspace_id,
        key_name,
        wrapped_dek.ciphertext,
        wrapped_dek.nonce[0..],
        wrapped_dek.tag[0..],
        encrypted_payload.nonce[0..],
        encrypted_payload.ciphertext,
        encrypted_payload.tag[0..],
        KEK_VERSION_AAD_BOUND,
        now_ms,
    });
    // info (not debug) by design: credential store/retrieve stays visible in default prod logs for
    // security-access monitoring — key_name only, never the secret value. LOGGING_STANDARD §4 exception.
    log.info("stored", .{ .workspace_id = workspace_id, .key_name = key_name });
}

/// Load and decrypt a secret from vault.secrets.
pub fn load(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ![]u8 {
    var result = PgQuery.from(try conn.query(sql.SELECT_SECRET, .{ workspace_id, key_name }));
    defer result.deinit();

    const row = try result.next() orelse {
        // Not-found is a normal control-flow path — caller decides whether to treat
        // it as an error. Log at debug so it doesn't trip "logged errors" test gates.
        log.debug("not_found", .{ .workspace_id = workspace_id, .key_name = key_name });
        return cp.SecretError.NotFound;
    };

    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);
    return decryptRowAt(alloc, row, workspace_id, key_name, &kek, 0);
}

/// Decrypt one `vault.secrets` row into plaintext, reading its ciphertext
/// columns starting at `col`. Caller owns the result and frees it through
/// `secure_memory.freeBytes`.
///
/// Split out of `load` so a workspace-wide read can decrypt many rows against
/// ONE query and ONE unwrapped Key Encryption Key (KEK). Every row-backed slice
/// is copied before returning, because the driver invalidates them on the next
/// `next()` — the caller may advance the cursor immediately after this returns.
fn decryptRowAt(
    alloc: std.mem.Allocator,
    row: anytype,
    workspace_id: []const u8,
    key_name: []const u8,
    kek: *const [KEY_LEN]u8,
    col: usize,
) ![]u8 {
    const encrypted_dek = try row.get([]u8, col);
    const dek_nonce_slice = try row.get([]u8, col + 1);
    const dek_tag_slice = try row.get([]u8, col + 2);
    const payload_nonce_slice = try row.get([]u8, col + 3);
    const payload_ciphertext = try row.get([]u8, col + 4);
    const payload_tag_slice = try row.get([]u8, col + 5);
    const kek_version = try row.get(i32, col + 6);
    if (kek_version != KEK_VERSION_LEGACY and kek_version != KEK_VERSION_AAD_BOUND) {
        log.err("unsupported_kek_version", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .kek_version = kek_version,
            .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
        });
        return cp.SecretError.UnsupportedKekVersion;
    }

    const dek_nonce = try cp.toFixed(NONCE_LEN, dek_nonce_slice);
    const dek_tag = try cp.toFixed(TAG_LEN, dek_tag_slice);
    const payload_nonce = try cp.toFixed(NONCE_LEN, payload_nonce_slice);
    const payload_tag = try cp.toFixed(TAG_LEN, payload_tag_slice);
    const ciphertext_copy = try alloc.dupe(u8, payload_ciphertext);
    defer alloc.free(ciphertext_copy);
    const dek_copy = try alloc.dupe(u8, encrypted_dek);
    defer alloc.free(dek_copy);

    const aad = if (kek_version == KEK_VERSION_AAD_BOUND)
        try buildAad(alloc, workspace_id, key_name, kek_version)
    else
        try alloc.dupe(u8, "");
    defer alloc.free(aad);

    const dek_plain = try cp.decrypt(alloc, &dek_nonce, dek_copy, &dek_tag, aad, kek);
    defer {
        std.crypto.secureZero(u8, dek_plain);
        alloc.free(dek_plain);
    }

    var dek = try cp.toFixed(KEY_LEN, dek_plain);
    defer std.crypto.secureZero(u8, &dek);
    const plaintext_result = cp.decrypt(alloc, &payload_nonce, ciphertext_copy, &payload_tag, aad, &dek) catch |err| {
        log.err("decrypt_failed", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
        });
        return err;
    };
    // info (not debug) by design — security-access visibility, see store() above (§4 exception).
    log.info("retrieved", .{ .workspace_id = workspace_id, .key_name = key_name });
    return plaintext_result;
}

/// One decrypted credential from a workspace-wide read. `plaintext` is
/// caller-owned and MUST be released through `freeEntries` (or an equivalent
/// zeroing free) — it is secret material, not ordinary heap.
pub const WorkspaceSecret = struct {
    key_name: []const u8,
    created_at: i64,
    plaintext: []u8,
};

/// Every credential in a workspace, decrypted, in ONE query and ONE KEK unwrap.
///
/// The per-key alternative issued a query per credential, so listing a
/// workspace cost a round trip per stored secret. Decryption is pure computation
/// once the row is in hand, so nothing here needs a second statement while the
/// cursor is open — which is what made the per-row form necessary before.
///
/// Ownership is all-or-nothing: on any error every plaintext already decrypted
/// is zeroed and freed before returning, so a partial failure never strands
/// secret material on the heap.
pub fn loadAllForWorkspace(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
) ![]WorkspaceSecret {
    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);

    var out: std.ArrayList(WorkspaceSecret) = .empty;
    // Elements only here: `out.items` aliases the list's own buffer, which
    // `deinit` releases. `freeEntries` additionally frees the slice and is for
    // the caller, who owns it after `toOwnedSlice`.
    errdefer {
        freeEntryContents(alloc, out.items);
        out.deinit(alloc);
    }

    var result = PgQuery.from(try conn.query(sql.SELECT_SECRETS_FOR_WORKSPACE, .{workspace_id}));
    defer result.deinit();
    while (try result.next()) |row| {
        const key_name = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(key_name);
        const created_at = try row.get(i64, 1);
        // Ciphertext columns start at index 2 — same block, same order as
        // SELECT_SECRET, which is why one decrypt routine serves both.
        const plaintext = try decryptRowAt(alloc, row, workspace_id, key_name, &kek, 2);
        errdefer secure_memory.freeBytes(alloc, plaintext);
        try out.append(alloc, .{ .key_name = key_name, .created_at = created_at, .plaintext = plaintext });
    }
    log.info("retrieved_workspace", .{ .workspace_id = workspace_id, .count = out.items.len });
    return out.toOwnedSlice(alloc);
}

/// Zero and release every plaintext in `entries`, their key names, and the
/// slice itself — `loadAllForWorkspace` hands back owned memory at both levels,
/// so releasing only the elements strands the backing array.
pub fn freeEntries(alloc: std.mem.Allocator, entries: []WorkspaceSecret) void {
    freeEntryContents(alloc, entries);
    alloc.free(entries);
}

/// Zero and release each entry's secret material, leaving the slice alone.
fn freeEntryContents(alloc: std.mem.Allocator, entries: []WorkspaceSecret) void {
    for (entries) |e| {
        secure_memory.freeBytes(alloc, e.plaintext);
        alloc.free(e.key_name);
    }
}
