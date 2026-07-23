//! Workspace credential list projection.
//!
//! ONE read over one `pg.Conn`: `crypto_store.loadAllForWorkspace` returns every
//! stored credential decrypted, and each body is projected to its non-secret
//! descriptors via `secret_metadata`. The api_key is never read. A row that
//! fails to parse degrades to an opaque `custom_secret` so the list still
//! returns 200.
//!
//! This was two passes — keys, then a decrypt-load per key — because a per-row
//! load cannot run while another result is open on the same connection. The
//! bulk read removes the constraint along with the per-credential round trip.

const std = @import("std");
const pg = @import("pg");
const crypto_store = @import("../../../secrets/crypto_store.zig");
const secret_metadata = @import("secret_metadata.zig");

/// One wire row. `kind` is a static `@tagName` slice (never freed); `name` and
/// the descriptors are heap-owned (see `freeRow`). No `api_key` field exists.
pub const SecretListRow = struct {
    name: []const u8,
    created_at: i64,
    kind: []const u8,
    provider: ?[]const u8 = null,
    model: ?[]const u8 = null,
    base_url: ?[]const u8 = null,
};

/// Project every workspace credential into its non-secret list row. Caller owns
/// the returned slice; on the request arena it is reclaimed wholesale.
pub fn fetchSecretListOnConn(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8) ![]SecretListRow {
    // One read for the whole workspace. This used to be two passes — keys, then
    // a decrypt-load per key — because a per-row load cannot run while another
    // result is open on the same connection. Decryption needs no statement of
    // its own, so the bulk read collapses both passes into one round trip
    // whose cost no longer tracks the number of stored credentials.
    const entries = try crypto_store.loadAllForWorkspace(alloc, conn, workspace_id);
    defer crypto_store.freeEntries(alloc, entries);

    var rows: std.ArrayList(SecretListRow) = .empty;
    errdefer {
        for (rows.items) |r| freeRow(alloc, r);
        rows.deinit(alloc);
    }
    for (entries) |e| {
        const row = try projectEntry(alloc, e);
        errdefer freeRow(alloc, row);
        try rows.append(alloc, row);
    }
    return rows.toOwnedSlice(alloc);
}

fn projectEntry(alloc: std.mem.Allocator, e: crypto_store.WorkspaceSecret) !SecretListRow {
    const name = try alloc.dupe(u8, e.key_name);
    errdefer alloc.free(name);

    // Legacy/corrupt body → opaque custom_secret; the list still returns 200.
    var parsed = parseObject(alloc, e.plaintext) catch {
        return .{ .name = name, .created_at = e.created_at, .kind = secret_metadata.Kind.custom_secret.wire() };
    };
    defer parsed.deinit();

    const p = secret_metadata.project(parsed.value);
    const provider = try dupeOpt(alloc, p.provider);
    errdefer if (provider) |v| alloc.free(v);
    const model = try dupeOpt(alloc, p.model);
    errdefer if (model) |v| alloc.free(v);
    const base_url = try dupeOpt(alloc, p.base_url);
    errdefer if (base_url) |v| alloc.free(v);

    return .{
        .name = name,
        .created_at = e.created_at,
        .kind = p.kind.wire(),
        .provider = provider,
        .model = model,
        .base_url = base_url,
    };
}

/// Parse a decrypted body, rejecting anything that is not a JSON object — the
/// same shape gate `vault.loadJson` applies, kept here now that the list reads
/// ciphertext in bulk rather than through the per-key vault helper.
fn parseObject(alloc: std.mem.Allocator, body: []const u8) !std.json.Parsed(std.json.Value) {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    if (parsed.value != .object) {
        parsed.deinit();
        return error.NotAnObject;
    }
    return parsed;
}

fn dupeOpt(alloc: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try alloc.dupe(u8, v) else null;
}

fn freeRow(alloc: std.mem.Allocator, r: SecretListRow) void {
    alloc.free(r.name);
    if (r.provider) |v| alloc.free(v);
    if (r.model) |v| alloc.free(v);
    if (r.base_url) |v| alloc.free(v);
    // r.kind is a static @tagName slice — not owned, never freed.
}
