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
const metadata = @import("../secrets/metadata.zig");
const sql = @import("../secrets/sql.zig");
const secure_memory = @import("../secrets/secure_memory.zig");
const error_codes = @import("../errors/error_registry.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");

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
/// pre-flight size check). Skips `validateObject` and re-stringification; the
/// caller is responsible for ensuring `plaintext` decodes to a non-empty JSON
/// object.
///
/// Derives the non-secret projection here and hands it to `crypto_store.store`,
/// which writes it in the SAME statement as the envelope. This is the only
/// place a projection is produced on the write path, and it produces it from
/// the exact bytes being encrypted — so the `meta_*` columns cannot come to
/// describe a body other than the one stored beside them.
///
/// The parse this adds is deliberate. It costs one JSON decode on a cold path
/// (credential create/update) and buys the read path up to one AES-GCM open per
/// row on every page view. A caller-supplied projection would skip the parse and
/// reintroduce exactly the drift this design exists to make impossible.
///
/// A body that does not parse is stored and projected as an opaque
/// `custom_secret` rather than rejected: `storeJsonPlaintext` has always skipped
/// the shape gate by design (the redaction harness stores non-JSON on purpose),
/// and failing here would change that behaviour for a reason unrelated to the
/// metadata promotion.
pub fn storeJsonPlaintext(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    return writeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext, crypto_store.store);
}

/// Replace the whole body of a secret this workspace already holds, deriving
/// the projection from the same bytes being encrypted.
///
/// `error.NotFound` when no such name is held — the write is an UPDATE, so it
/// creates nothing. Replacement is total: a field absent from `plaintext` is
/// absent from the stored secret afterwards. That is the point of the verb, and
/// it is why no caller needs to read a secret back in order to change it.
pub fn replaceJsonPlaintext(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    return writeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext, crypto_store.replace);
}

/// Same projection derivation as `storeJsonPlaintext`, but the write claims a
/// free name instead of overwriting whatever holds it — `error.SecretNameTaken`
/// when one already does.
///
/// The create route uses this and the replace route uses `replaceJsonPlaintext`;
/// the OAuth connector callbacks and the token refresh deliberately stay on the
/// overwriting `storeJsonPlaintext`, because re-connecting a provider is a
/// rotation.
pub fn createJsonPlaintext(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    return writeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext, crypto_store.create);
}

fn writeJsonPlaintext(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    comptime write: fn (std.mem.Allocator, *pg.Conn, []const u8, []const u8, []const u8, metadata.Projection) anyerror!void,
) !void {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, plaintext, .{}) catch {
        try write(alloc, conn, workspace_id, key_name, plaintext, .{ .kind = .custom_secret });
        return;
    };
    defer parsed.deinit();
    try write(alloc, conn, workspace_id, key_name, plaintext, metadata.project(parsed.value));
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
    // Presence still requires SELECT on the table, which only `vault_runtime`
    // holds (schema/300); the result drains (defer) before the commit.
    var scope = try pool_elevation.begin(conn, .vault);
    defer scope.deinit();
    {
        var q = PgQuery.from(try scope.query(
            \\SELECT key_name FROM vault.secrets WHERE workspace_id = $1 AND key_name = ANY($2::text[])
        , .{ workspace_id, candidates }));
        defer q.deinit();
        while (try q.next()) |row| {
            const found = try row.get([]const u8, 0);
            // candidates is tiny (≤ the registry size); a linear match is trivial and
            // avoids allocating/duping the borrowed row key into a set.
            for (candidates, 0..) |cand, i| {
                if (std.mem.eql(u8, cand, found)) present_out[i] = true;
            }
        }
    }
    try scope.commit();
}

/// One credential's non-secret projection, read from columns rather than
/// derived from ciphertext. Strings are owned by the allocator passed to
/// `loadMetadata`; release through `freeMetadata`.
///
/// There is no `api_key` field and no way to add one — the columns behind this
/// struct hold no key material (`schema/036_vault_secret_metadata.sql`), so the
/// read path has nothing to leak even if a future projection is careless.
pub const SecretMetadata = struct {
    kind: metadata.Kind,
    provider: ?[]const u8 = null,
    base_url: ?[]const u8 = null,
    has_key: bool = false,

    pub fn deinit(self: *const SecretMetadata, alloc: std.mem.Allocator) void {
        if (self.provider) |p| alloc.free(p);
        if (self.base_url) |b| alloc.free(b);
    }
};

/// The non-secret projection for each of `candidates`, in ONE query that NEVER
/// decrypts — the metadata sibling of `markExisting` (the never-decrypt invariant).
///
/// `out[i]` is set for each `candidates[i]` that has a row and left `null` for
/// each that does not, so presence is `out[i] != null` and needs no second
/// query. `out.len` MUST equal `candidates.len`. The caller owns every non-null
/// entry and releases the set through `freeMetadata`.
///
/// Both this and `markExisting` exist on purpose. `markExisting` selects one
/// column for callers that only ask "is it there?" (the connector catalog's
/// configured/connected flags); collapsing the two would make every presence
/// check carry four columns it discards. Two questions, two statements, one
/// index.
///
/// A row written before `schema/036` has NULL metadata and reports as an opaque
/// `custom_secret` with no key. It is NOT healed by decrypting here: a
/// heal-on-read path would put an envelope open back on the read path and make
/// "library reads never decrypt" true only after warm-up. `agentsfleetd
/// backfill` is what fills those rows.
pub fn loadMetadata(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    candidates: []const []const u8,
    out: []?SecretMetadata,
) !void {
    std.debug.assert(out.len == candidates.len);
    @memset(out, null);
    if (candidates.len == 0) return;
    errdefer freeMetadata(alloc, out);

    // The projection columns hold no key material, but the table's SELECT
    // belongs to `vault_runtime` alone (schema/300). `out` fills in the
    // caller's frame — the errdefer above owns it on any failure, the
    // commit's included.
    var scope = try pool_elevation.begin(conn, .vault);
    defer scope.deinit();
    {
        var q = PgQuery.from(try scope.query(sql.SELECT_METADATA_FOR_KEYS, .{ workspace_id, candidates }));
        defer q.deinit();
        while (try q.next()) |row| {
            const found = try row.get([]const u8, 0);
            // EVERY matching slot is filled, not just the first. Callers pass a
            // positional list rather than a deduplicated set — one credential backs
            // several model rows, and letting `out[i]` belong to `candidates[i]` by
            // construction is cheaper than deduplicating and then matching back.
            // Each duplicate gets its OWN owned copy, so `freeMetadata` releases
            // them independently and no slot aliases another's strings.
            //
            // Linear match, mirroring markExisting: `candidates` is bounded by the
            // page limit, and comparing is cheaper than allocating a set and duping
            // the borrowed row key into it.
            for (candidates, 0..) |cand, i| {
                if (out[i] == null and std.mem.eql(u8, cand, found)) {
                    out[i] = try rowToMetadata(alloc, row);
                }
            }
        }
    }
    try scope.commit();
}

/// Release every projection in `out` and blank the slots, so a double call and a
/// partially-filled error path are both safe. The slice itself is the caller's.
pub fn freeMetadata(alloc: std.mem.Allocator, out: []?SecretMetadata) void {
    for (out) |*slot| {
        if (slot.*) |*m| m.deinit(alloc);
        slot.* = null;
    }
}

/// Map one `SELECT_METADATA_FOR_KEYS` row. A NULL or unrecognised `meta_kind`
/// degrades to `custom_secret` rather than failing the read: an un-backfilled
/// row and a row written by a newer binary are both "we cannot describe this",
/// and neither is a reason to fail a whole page.
fn rowToMetadata(alloc: std.mem.Allocator, row: anytype) !SecretMetadata {
    const kind: metadata.Kind = if (try row.get(?[]const u8, 1)) |text|
        std.meta.stringToEnum(metadata.Kind, text) orelse .custom_secret
    else
        .custom_secret;

    const provider = try dupeOpt(alloc, try row.get(?[]const u8, 2));
    errdefer if (provider) |p| alloc.free(p);
    const base_url = try dupeOpt(alloc, try row.get(?[]const u8, 3));
    errdefer if (base_url) |b| alloc.free(b);

    return .{
        .kind = kind,
        .provider = provider,
        .base_url = base_url,
        .has_key = (try row.get(?bool, 4)) orelse false,
    };
}

fn dupeOpt(alloc: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try alloc.dupe(u8, v) else null;
}

/// Hard-delete the row at (workspace_id, key_name). Idempotent: `true` if a
/// row was removed, `false` if nothing matched. Callers that expose this via
/// HTTP DELETE typically discard the return and respond 204 either way.
pub fn deleteCredential(
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) !bool {
    // DELETE belongs to `vault_runtime` (schema/300). Inside the secret
    // reference protocol's transaction the scope brackets just the statement.
    var scope = try pool_elevation.begin(conn, .vault);
    defer scope.deinit();
    const rowcount = try scope.exec(
        \\DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2
    , .{ workspace_id, key_name });
    try scope.commit();
    return (rowcount orelse 0) > 0;
}
