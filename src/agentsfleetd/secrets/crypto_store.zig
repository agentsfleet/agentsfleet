//! Database-backed secret storage using envelope encryption.
//! Depends on crypto_primitives for all crypto operations.

const std = @import("std");
const builtin = @import("builtin");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const cp = @import("crypto_primitives.zig");
const sql = @import("sql.zig");
const secure_memory = @import("secure_memory.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");

const log = logging.scoped(.secrets);

/// Test-only tally of envelope opens.
///
/// The library read paths must decrypt exactly zero times. Prose cannot enforce
/// that — the next handler to reach for `vault.loadJson` on a read would satisfy
/// every other gate we have. This turns the invariant into an assertion:
/// `test_library_reads_never_decrypt` drives the tenant registry, global models,
/// Fleet summary, and Fleet detail reads and requires the tally to still read
/// zero.
///
/// Atomic because integration tests exercise handlers concurrently; `.monotonic`
/// suffices, since assertions read the total after requests are joined, never
/// mid-flight. In a release build `noteDecrypt` compiles to nothing and this is
/// eight untouched bytes — not worth a comptime-branched wrapper type to elide.
var decrypt_tally: std.atomic.Value(usize) = .init(0);

inline fn noteDecrypt() void {
    if (comptime !builtin.is_test) return;
    _ = decrypt_tally.fetchAdd(1, .monotonic);
}

/// Envelope opens since the last `resetDecryptCountForTest`. Always 0 in a
/// release build.
pub fn decryptCountForTest() usize {
    return decrypt_tally.load(.monotonic);
}

/// Zero the tally, so a count is scoped to the request under test rather than to
/// the whole binary. Every test asserting on it calls this first.
pub fn resetDecryptCountForTest() void {
    decrypt_tally.store(0, .monotonic);
}

const KEY_LEN = cp.KEY_LEN;
const NONCE_LEN = cp.NONCE_LEN;
const TAG_LEN = cp.TAG_LEN;
// The write path lives in `crypto_store_write.zig` (RULE FLL split at the
// read/write seam); this module stays the one import surface for both sides.
const write_path = @import("crypto_store_write.zig");
pub const store = write_path.store;
pub const create = write_path.create;
pub const replace = write_path.replace;
const KEK_VERSION_AAD_BOUND = write_path.KEK_VERSION_AAD_BOUND;
const buildAad = write_path.buildAad;

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
    // Every stored row is kek_version 2 — the DB CHECK (schema/039) makes any
    // other value impossible, so there is no version to branch on. The AAD
    // binds that version too, so a row that somehow held a different one would
    // fail the AEAD tag rather than decrypt (openEnvelopeAt → DecryptFailed).
    const aad = try buildAad(alloc, workspace_id, key_name, KEK_VERSION_AAD_BOUND);
    defer alloc.free(aad);
    return openEnvelopeAt(alloc, row, workspace_id, key_name, kek, col, aad);
}

/// The byte work of one envelope open: read the six ciphertext columns at
/// `col`, unwrap the Data Encryption Key (DEK) under `aad`, decrypt the
/// payload. Caller owns the result (free via `secure_memory.freeBytes`) and
/// supplies the AAD.
fn openEnvelopeAt(
    alloc: std.mem.Allocator,
    row: anytype,
    workspace_id: []const u8,
    key_name: []const u8,
    kek: *const [KEY_LEN]u8,
    col: usize,
    aad: []const u8,
) ![]u8 {
    // The single funnel every envelope open passes through — `load` and
    // `loadAllForWorkspace` both land here, so no caller decrypts untallied. Counted on ENTRY, not on success: a read path that
    // touched ciphertext and then failed still touched ciphertext, and the
    // invariant is about touching.
    noteDecrypt();

    const encrypted_dek = try row.get([]u8, col);
    const dek_nonce_slice = try row.get([]u8, col + 1);
    const dek_tag_slice = try row.get([]u8, col + 2);
    const payload_nonce_slice = try row.get([]u8, col + 3);
    const payload_ciphertext = try row.get([]u8, col + 4);
    const payload_tag_slice = try row.get([]u8, col + 5);

    const dek_nonce = try cp.toFixed(NONCE_LEN, dek_nonce_slice);
    const dek_tag = try cp.toFixed(TAG_LEN, dek_tag_slice);
    const payload_nonce = try cp.toFixed(NONCE_LEN, payload_nonce_slice);
    const payload_tag = try cp.toFixed(TAG_LEN, payload_tag_slice);
    const ciphertext_copy = try alloc.dupe(u8, payload_ciphertext);
    defer alloc.free(ciphertext_copy);
    const dek_copy = try alloc.dupe(u8, encrypted_dek);
    defer alloc.free(dek_copy);

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

/// One credential from a workspace-wide read. `plaintext` is null when THIS
/// row's envelope could not be decrypted — a damaged or legacy row does not
/// fail the whole read, matching the per-row degradation the list had before
/// the bulk path. When non-null it is caller-owned secret material and MUST be
/// released through `freeEntries` (or an equivalent zeroing free).
pub const WorkspaceSecret = struct {
    key_name: []const u8,
    created_at: i64,
    plaintext: ?[]u8,
};

/// Every credential in a workspace, decrypted, in ONE query and ONE KEK unwrap.
///
/// The per-key alternative issued a query per credential, so listing a
/// workspace cost a round trip per stored secret. Decryption is pure computation
/// once the row is in hand, so nothing here needs a second statement while the
/// cursor is open — which is what made the per-row form necessary before.
///
/// Row isolation is preserved: a single undecryptable envelope degrades to a
/// null `plaintext` for that row, it does NOT abort the read. The per-key path
/// this replaced caught a decrypt failure per row and degraded it to an opaque
/// entry so the list still returned 200; collapsing to one query must not
/// collapse that isolation. A transport or protocol error (not a per-row
/// decrypt failure) still propagates, and every plaintext already decrypted is
/// zeroed and freed before returning so no secret material is stranded.
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
    var undecryptable: usize = 0;
    while (try result.next()) |row| {
        const key_name = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(key_name);
        const created_at = try row.get(i64, 1);
        // Ciphertext columns start at index 2 — same block, same order as
        // SELECT_SECRET, which is why one decrypt routine serves both. A decrypt
        // failure degrades THIS row to null rather than failing the workspace;
        // decryptRowAt has already logged the cause with row context.
        const plaintext = decryptRowAt(alloc, row, workspace_id, key_name, &kek, 2) catch |err| blk: {
            if (err == error.OutOfMemory) return err; // not a per-row data fault
            undecryptable += 1;
            break :blk null;
        };
        errdefer if (plaintext) |p| secure_memory.freeBytes(alloc, p);
        try out.append(alloc, .{ .key_name = key_name, .created_at = created_at, .plaintext = plaintext });
    }
    log.info("retrieved_workspace", .{ .workspace_id = workspace_id, .count = out.items.len, .undecryptable = undecryptable });
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
        if (e.plaintext) |p| secure_memory.freeBytes(alloc, p);
        alloc.free(e.key_name);
    }
}
