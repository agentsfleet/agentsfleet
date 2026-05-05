//! Structured-credential layer over crypto_store.
//!
//! `vault.secrets` already KMS-envelopes opaque bytes; this module makes those
//! bytes a non-empty JSON object so a single credential can carry multiple
//! named fields (e.g. `{host, api_token}`) addressable as
//! `${secrets.<name>.<field>}` at the tool bridge.
//!
//! Callers own the storage key string. The wrapper does not compose a prefix —
//! the handler that calls into this module decides whether the row is a
//! zombie credential (`zombie:<name>`), a BYOK provider record (user-named),
//! or anything else. Keeps this layer reusable without coupling to a single
//! caller's naming convention.

const std = @import("std");
const pg = @import("pg");
const crypto_store = @import("../secrets/crypto_store.zig");

const log = std.log.scoped(.vault);

pub const Error = error{
    /// Caller passed a non-object JSON value (string/array/number/bool/null).
    NotAnObject,
    /// Caller passed `{}` — operator forgot to populate fields.
    EmptyObject,
    /// Decryption succeeded but the plaintext was not parseable JSON, or
    /// parsed to a non-object value. Surfaces only on rows that bypassed
    /// `storeJson` (e.g. legacy `--value` rows or DB corruption).
    MalformedPlaintext,
};

/// Encrypt and persist `value` as the canonical-stringified JSON object for
/// (workspace_id, key_name). Rejects non-object and empty-object inputs at
/// the API boundary so we never store ambiguous shapes.
///
/// Pure shape gate — exposed so unit tests can exercise rejection branches
/// without spinning up a DB. Mirrors the checks `storeJson` runs before
/// touching the connection.
pub fn validateObject(value: std.json.Value) Error!void {
    if (value != .object) return Error.NotAnObject;
    if (value.object.count() == 0) return Error.EmptyObject;
}

pub fn storeJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    value: std.json.Value,
) !void {
    try validateObject(value);

    const plaintext = try std.json.Stringify.valueAlloc(alloc, value, .{});
    defer alloc.free(plaintext);

    try storeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext);
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
/// guaranteed to be `.object` — `storeJson` rejects everything else, and any
/// non-object plaintext discovered at load time surfaces as
/// `Error.MalformedPlaintext` rather than a silent success.
pub fn loadJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) !std.json.Parsed(std.json.Value) {
    const plaintext = try crypto_store.load(alloc, conn, workspace_id, key_name);
    defer alloc.free(plaintext);

    // Log at warn (not err) so the negative-path test that deliberately
    // writes a non-JSON plaintext does not trip the "logged errors" test
    // gate. Operators still see the line; it just doesn't break CI.
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, plaintext, .{}) catch |err| {
        log.warn("vault.malformed_plaintext workspace_id={s} key_name={s} parse_err={s}", .{
            workspace_id, key_name, @errorName(err),
        });
        return Error.MalformedPlaintext;
    };
    if (parsed.value != .object) {
        parsed.deinit();
        log.warn("vault.malformed_plaintext_not_object workspace_id={s} key_name={s}", .{ workspace_id, key_name });
        return Error.MalformedPlaintext;
    }
    return parsed;
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
