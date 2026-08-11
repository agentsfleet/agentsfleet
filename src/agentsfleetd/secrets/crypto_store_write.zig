//! The vault envelope write path: seal plaintext under a fresh Data
//! Encryption Key (DEK), wrap that DEK under the process Key-Encryption Key
//! (KEK), and land envelope + non-secret projection in ONE statement.
//!
//! Split from `crypto_store.zig` at the read/write seam (RULE FLL): that
//! module remains the public API and re-exports `store`/`create`/`replace`;
//! this one owns the AAD construction both sides share.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const pg = @import("pg");
const pool_elevation = @import("../db/pool_elevation.zig");
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");
const sql = @import("sql.zig");
const metadata = @import("metadata.zig");
const logging = @import("log");

const log = logging.scoped(.secrets);

const KEY_LEN = cp.KEY_LEN;

// There is exactly one envelope version. Version 1 (sealed before AAD binding,
// pre-`0ff4902ca`) is not supported: nothing writes it and nothing reads it —
// a surviving row answers the typed unsupported-version error at use, and its
// owner replaces the secret.
pub const KEK_VERSION_AAD_BOUND: i32 = 2;
const AAD_SEPARATOR: u8 = 0x1f;
const AAD_FORMAT = "{s}{c}{s}{c}{d}";

pub fn buildAad(alloc: std.mem.Allocator, workspace_id: []const u8, key_name: []const u8, kek_version: i32) ![]u8 {
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
    return writeEnvelope(alloc, conn, workspace_id, key_name, plaintext, projection, sql.INSERT_SECRET, true, error.SecretNameTaken);
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
    return writeEnvelope(alloc, conn, workspace_id, key_name, plaintext, projection, sql.INSERT_SECRET_IF_ABSENT, true, error.SecretNameTaken);
}

/// Replace the body of a secret this workspace already holds.
///
/// `error.NotFound` when it holds no such name: the statement is an UPDATE, so
/// zero affected rows is the answer and nothing is created. That is what keeps
/// a replace racing a delete from resurrecting the credential, and what keeps
/// claiming a name `create`'s job alone.
pub fn replace(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    projection: metadata.Projection,
) !void {
    return writeEnvelope(alloc, conn, workspace_id, key_name, plaintext, projection, sql.UPDATE_SECRET, false, error.NotFound);
}

/// Seal `plaintext` into a fresh envelope and run `statement`.
///
/// `with_id` selects the parameter list: the insert arms mint and bind a new
/// row id, while the UPDATE arm keeps the row's existing one and binds neither
/// it nor `created_at`. `zero_rows_err` is what an affected-row count of zero
/// means for this caller — a taken name for `create`, a missing one for
/// `replace` — because the count is the answer in both directions and neither
/// caller may re-read to find out.
/// The envelope: the payload under a per-secret Data Encryption Key (DEK), and
/// that DEK under the process Key Encryption Key (KEK). Owns both buffers.
const Sealed = struct {
    wrapped_dek: cp.EncryptedBlob,
    encrypted_payload: cp.EncryptedBlob,

    fn deinit(self: *Sealed, alloc: std.mem.Allocator) void {
        self.wrapped_dek.deinit(alloc);
        self.encrypted_payload.deinit(alloc);
        self.* = undefined;
    }
};

/// Derive a fresh DEK and seal both halves. Runs entirely before any
/// elevation, so no transaction is ever held open across key derivation.
fn seal(
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !Sealed {
    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);

    var dek: [KEY_LEN]u8 = undefined;
    defer std.crypto.secureZero(u8, &dek);
    try constants.secureRandomBytes(&dek);

    // Both halves bind the same Additional Authenticated Data (AAD), so a
    // ciphertext moved to another workspace or key name fails to open.
    const aad = try buildAad(alloc, workspace_id, key_name, KEK_VERSION_AAD_BOUND);
    defer alloc.free(aad);

    const wrapped_dek = try cp.encrypt(alloc, dek[0..], aad, &kek);
    errdefer wrapped_dek.deinit(alloc);
    const encrypted_payload = try cp.encrypt(alloc, plaintext, aad, &dek);
    return .{ .wrapped_dek = wrapped_dek, .encrypted_payload = encrypted_payload };
}

/// One sealed row's bound values, held together so the two argument shapes
/// below read as the same write with and without a generated identifier.
/// The blobs are borrowed — they outlive the elevated span on the caller's
/// stack, which is where their `defer` frees them.
const SealedWrite = struct {
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    key_name: []const u8,
    wrapped_dek: *const cp.EncryptedBlob,
    encrypted_payload: *const cp.EncryptedBlob,
    projection: metadata.Projection,
    now_ms: i64,
    statement: []const u8,

    /// `create`: the row identifier is generated here, so the statement binds
    /// it first and every later placeholder shifts by one.
    fn execCreate(c: SealedWrite, v: pool_elevation.Elevated(.vault)) !?i64 {
        const secret_id = try id_format.generateVaultSecretId(c.alloc);
        defer c.alloc.free(secret_id);
        return v.conn.exec(c.statement, .{
            secret_id,
            c.workspace_id,
            c.key_name,
            c.wrapped_dek.ciphertext,
            c.wrapped_dek.nonce[0..],
            c.wrapped_dek.tag[0..],
            c.encrypted_payload.nonce[0..],
            c.encrypted_payload.ciphertext,
            c.encrypted_payload.tag[0..],
            KEK_VERSION_AAD_BOUND,
            c.now_ms,
            c.projection.kind.wire(),
            c.projection.provider,
            c.projection.base_url,
            c.projection.has_key,
        });
    }

    /// `replace`: the row already exists and is matched by workspace and name.
    fn execReplace(c: SealedWrite, v: pool_elevation.Elevated(.vault)) !?i64 {
        return v.conn.exec(c.statement, .{
            c.workspace_id,
            c.key_name,
            c.wrapped_dek.ciphertext,
            c.wrapped_dek.nonce[0..],
            c.wrapped_dek.tag[0..],
            c.encrypted_payload.nonce[0..],
            c.encrypted_payload.ciphertext,
            c.encrypted_payload.tag[0..],
            KEK_VERSION_AAD_BOUND,
            c.now_ms,
            c.projection.kind.wire(),
            c.projection.provider,
            c.projection.base_url,
            c.projection.has_key,
        });
    }
};

fn writeEnvelope(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    projection: metadata.Projection,
    statement: []const u8,
    comptime with_id: bool,
    comptime zero_rows_err: anyerror,
) !void {
    var sealed = try seal(alloc, workspace_id, key_name, plaintext);
    defer sealed.deinit(alloc);

    // The write lands only as `vault_runtime` (schema/300). Inside a caller's
    // transaction (the secret-reference protocol) the callback brackets just
    // the statement; standalone it owns the transaction. All envelope crypto
    // above runs BEFORE elevating, so no transaction is held open across key
    // derivation — the elevated span is one INSERT/UPDATE.
    const written = try pool_elevation.withRole(conn, .vault, SealedWrite{
        .alloc = alloc,
        .workspace_id = workspace_id,
        .key_name = key_name,
        .wrapped_dek = &sealed.wrapped_dek,
        .encrypted_payload = &sealed.encrypted_payload,
        .projection = projection,
        .now_ms = clock.nowMillis(),
        .statement = statement,
    }, struct {
        fn run(c: SealedWrite, v: pool_elevation.Elevated(.vault)) !?i64 {
            return if (with_id) c.execCreate(v) else c.execReplace(v);
        }
    }.run);
    // The affected-row count is the answer, and it means opposite things to the
    // two callers: zero rows is a taken name for `create` (DO NOTHING wrote
    // nothing) and a missing one for `replace` (the UPDATE matched nothing).
    // Neither may re-read to find out — that read is the race each statement
    // exists to avoid — so the count decides and the caller names the error.
    if ((written orelse 0) == 0) return zero_rows_err;
    // info (not debug) by design: credential store/retrieve stays visible in default prod logs for
    // security-access monitoring — key_name only, never the secret value. LOGGING_STANDARD §4 exception.
    log.info("stored", .{ .workspace_id = workspace_id, .key_name = key_name });
}
