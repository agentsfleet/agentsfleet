//! Database-backed secret storage using envelope encryption.
//! Depends on crypto_primitives for all crypto operations.

const std = @import("std");
const builtin = @import("builtin");
const constants = @import("common");
const clock = constants.clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");
const sql = @import("sql.zig");
const metadata = @import("metadata.zig");
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
const KEK_VERSION_LEGACY: i32 = 1;
const KEK_VERSION_AAD_BOUND: i32 = 2;
const AAD_SEPARATOR: u8 = 0x1f;
const AAD_FORMAT = "{s}{c}{s}{c}{d}";

fn buildAad(alloc: std.mem.Allocator, workspace_id: []const u8, key_name: []const u8, kek_version: i32) ![]u8 {
    const canonical_workspace_id = try std.ascii.allocLowerString(alloc, workspace_id);
    defer alloc.free(canonical_workspace_id);
    return std.fmt.allocPrint(alloc, AAD_FORMAT, .{ canonical_workspace_id, AAD_SEPARATOR, key_name, AAD_SEPARATOR, kek_version });
}

/// Store encrypted secret in vault.secrets with envelope encryption, together
/// with the non-secret projection of the body being stored (the metadata promotion).
///
/// `projection` is not optional and carries no default. Every caller must have
/// derived it from THIS `plaintext`, because the two are written by one
/// statement and a mismatched pair is indistinguishable afterwards from a
/// correct one. Making it a required parameter is the enforcement: a caller that
/// has not looked at the body it is storing cannot call this.
pub fn store(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    projection: metadata.Projection,
) !void {
    return writeEnvelope(alloc, conn, workspace_id, key_name, plaintext, projection, sql.INSERT_SECRET);
}

/// Store a credential under a name nobody holds yet, or fail with
/// `error.SecretNameTaken` having written nothing.
///
/// The uniqueness decision belongs to the database, not to a caller that read
/// the name list first: two creates racing on one name would both find it free.
/// Rotation is `store`, which is a different verb on a different route.
pub fn create(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    projection: metadata.Projection,
) !void {
    return writeEnvelope(alloc, conn, workspace_id, key_name, plaintext, projection, sql.INSERT_SECRET_IF_ABSENT);
}

fn writeEnvelope(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    projection: metadata.Projection,
    statement: []const u8,
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
    const written = try conn.exec(statement, .{
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
        projection.kind.wire(),
        projection.provider,
        projection.base_url,
        projection.has_key,
    });
    // Only the create statement can decline — the rotate arm always updates, so
    // it reports one row here. An absent count is treated as "not written": for
    // a credential we would rather answer "that name is taken" than report a
    // success we cannot confirm, and `DO NOTHING` guarantees nothing was
    // overwritten either way.
    if ((written orelse 0) == 0) return error.SecretNameTaken;
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
    // The single funnel every envelope open passes through — `load` and
    // `loadAllForWorkspace` both land here, so no caller decrypts untallied.
    // Counted on ENTRY, not on success: a read path that touched ciphertext and
    // then failed still touched ciphertext, and the invariant is about touching.
    noteDecrypt();

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
