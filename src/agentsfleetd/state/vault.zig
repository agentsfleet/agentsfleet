//! Structured-credential layer over crypto_store.
//!
//! `vault.secrets` already KMS-envelopes opaque bytes; this module makes those
//! bytes a non-empty JSON object so a single credential can carry multiple
//! named fields (e.g. `{host, api_token}`) addressable as
//! `${secrets.<name>.<field>}` at the tool bridge.
//!
//! Callers own the storage key string. The wrapper does not compose a prefix —
//! the handler that calls into this module decides whether the row is a
//! agent credential (`fleet:<name>`), a self-managed provider record (user-named),
//! or anything else. Keeps this layer reusable without coupling to a single
//! caller's naming convention.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const crypto_store = @import("../secrets/crypto_store.zig");
const secure_memory = @import("../secrets/secure_memory.zig");
const error_codes = @import("../errors/error_registry.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const log = logging.scoped(.vault);

pub const Error = error{
    /// Caller passed a non-object JSON value (string/array/number/bool/null).
    NotAnObject,
    /// Caller passed `{}` — operator forgot to populate fields.
    EmptyObject,
};

/// Encrypt and persist `value` as the canonical-stringified JSON object for
/// (workspace_id, key_name). Rejects non-object and empty-object inputs at
/// the API boundary so we never store ambiguous shapes.
///
/// Pure shape gate — exposed so unit tests can exercise rejection branches
/// without spinning up a DB, and so JSON writers validate before stringifying
/// into `storeJsonPlaintext`.
pub fn validateObject(value: std.json.Value) Error!void {
    if (value != .object) return Error.NotAnObject;
    if (value.object.count() == 0) return Error.EmptyObject;
}

/// Lower-level form for callers that already hold the canonical-stringified
/// JSON-object plaintext (e.g. an HTTP handler that stringified once for a
/// pre-flight size check). Skips `validateObject` and re-stringification on
/// the hot path; the caller is responsible for ensuring `plaintext` decodes
/// to a non-empty JSON object.
pub fn storeJsonPlaintext(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    try crypto_store.store(alloc, conn, workspace_id, key_name, plaintext);
}

/// Decrypt and parse the row at (workspace_id, key_name) as a JSON object.
///
/// Returns `std.json.Parsed(std.json.Value)`; the caller MUST call `.deinit()`
/// on the returned handle to free the parser arena. The wrapped `value` is
/// guaranteed to be `.object` — every writer routes through
/// `storeJsonPlaintext`, both of which run `validateObject` (directly or via
/// the caller's pre-flight) before the AES-GCM envelope, and the AEAD tag
/// rejects any tampered ciphertext at decrypt time.
pub fn loadJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) !std.json.Parsed(std.json.Value) {
    const plaintext = try crypto_store.load(alloc, conn, workspace_id, key_name);
    defer secure_memory.freeBytes(alloc, plaintext);

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, plaintext, .{}) catch |err| {
        // AEAD + validateObject make this unreachable for rows written via
        // storeJson. storeJsonPlaintext skips the shape gate by design, so a
        // malformed caller can still land bytes here. Warn (not err) so the
        // redaction harness's deliberate non-JSON plaintext fixture does
        // not trip the test runner's logged-errors gate; operators still
        // get workspace + key context to pinpoint the corrupt row.
        log.warn("vault_load_parse_failed", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .err = @errorName(err),
            .error_code = error_codes.ERR_VAULT_DATA_INVALID,
        });
        return err;
    };
    if (parsed.value != .object) {
        parsed.deinit();
        return Error.NotAnObject;
    }
    return parsed;
}

/// Which of `candidates` exist as rows for `workspace_id` — a batch existence
/// check that NEVER decrypts (unlike `loadJson`), for callers that only need
/// presence (e.g. the connector catalog's configured/connected flags). One
/// query instead of N `loadJson` decrypts. `present_out[i]` is set for each
/// `candidates[i]` that has a row; `present_out.len` MUST equal `candidates.len`.
pub fn markExisting(
    conn: *pg.Conn,
    workspace_id: []const u8,
    candidates: []const []const u8,
    present_out: []bool,
) !void {
    std.debug.assert(present_out.len == candidates.len);
    @memset(present_out, false);
    if (candidates.len == 0) return;
    var q = PgQuery.from(try conn.query(
        \\SELECT key_name FROM vault.secrets WHERE workspace_id = $1 AND key_name = ANY($2::text[])
    , .{ workspace_id, candidates }));
    defer q.deinit();
    while (try q.next()) |row| {
        const found = try row.get([]const u8, 0);
        // candidates is tiny (≤ the registry size); a linear match is trivial and
        // avoids allocating/duping the borrowed row key into a set.
        for (candidates, 0..) |c, i| {
            if (std.mem.eql(u8, c, found)) present_out[i] = true;
        }
    }
}

/// Hard-delete the row at (workspace_id, key_name). Idempotent: `true` if a
/// row was removed, `false` if nothing matched. Callers that expose this via
/// HTTP DELETE typically discard the return and respond 204 either way.
pub fn deleteCredential(
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) !bool {
    const rowcount = try conn.exec(
        \\DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2
    , .{ workspace_id, key_name });
    return (rowcount orelse 0) > 0;
}
