//! Webhook-sig lookup: resolves URL-secret, Bearer token, and per-zombie
//! HMAC signature scheme + secret for the webhook_sig middleware.
//! Lives in `src/cmd/` so it can import both `src/auth/` and `src/zombie/`.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const crypto_store = @import("../secrets/crypto_store.zig");
const webhook_verify = @import("../zombie/webhook_verify.zig");
const auth_mw = @import("../auth/middleware/mod.zig");

const LookupResult = auth_mw.webhook_sig_mod.LookupResult;
const SignatureScheme = auth_mw.webhook_sig_mod.SignatureScheme;
const SvixLookupResult = auth_mw.svix_signature_mod.SvixLookupResult;

const log = std.log.scoped(.webhook_sig_lookup);

pub fn lookup(
    pool: *pg.Pool,
    zombie_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?LookupResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const row_data = (try fetchZombieRow(conn, alloc, zombie_id)) orelse return null;
    defer freeRowData(alloc, row_data);

    var token: ?[]const u8 = null;
    errdefer if (token) |t| alloc.free(t);
    if (row_data.token_raw) |t| token = try alloc.dupe(u8, t);

    var expected_secret: ?[]const u8 = null;
    errdefer if (expected_secret) |s| alloc.free(s);
    if (row_data.url_secret_ref) |ref| {
        expected_secret = crypto_store.load(alloc, conn, row_data.workspace_id, ref) catch |err| blk: {
            log.err("vault_load_failed ref={s} err={s}", .{ ref, @errorName(err) });
            break :blk null;
        };
    }

    var scheme: ?SignatureScheme = null;
    var signature_secret: ?[]const u8 = null;
    errdefer if (scheme) |s| freeScheme(alloc, s);
    errdefer if (signature_secret) |s| alloc.free(s);
    if (row_data.signature_json) |sig_json| {
        if (try parseSignature(alloc, sig_json, row_data.source)) |p| {
            scheme = p.scheme;
            signature_secret = crypto_store.load(alloc, conn, row_data.workspace_id, p.secret_ref) catch |err| blk: {
                log.err("vault_load_failed signature_ref={s} err={s}", .{ p.secret_ref, @errorName(err) });
                break :blk null;
            };
            alloc.free(p.secret_ref);
        }
    }

    return .{
        .expected_secret = expected_secret,
        .expected_token = token,
        .signature_scheme = scheme,
        .signature_secret = signature_secret,
    };
}

/// M28_001 §5: Svix middleware lookup. Fetches the Clerk-style
/// `signature.secret_ref` from the zombie's config_json and resolves it to
/// the `whsec_<base64>` secret via the workspace vault. Middleware handles
/// prefix stripping + base64 decoding.
///
/// Uses `extractSecretRef` (scheme-free JSON parse) instead of the full
/// `parseSignature` path — Svix doesn't need scheme metadata, so skipping
/// the 4 dup'd strings + their frees saves per-request work.
pub fn lookupSvix(
    pool: *pg.Pool,
    zombie_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?SvixLookupResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const row_data = (try fetchZombieRow(conn, alloc, zombie_id)) orelse return null;
    defer freeRowData(alloc, row_data);

    const sig_json = row_data.signature_json orelse return .{ .secret = null };
    const secret_ref = (try extractSecretRef(alloc, sig_json)) orelse return .{ .secret = null };
    defer alloc.free(secret_ref);

    const secret = crypto_store.load(alloc, conn, row_data.workspace_id, secret_ref) catch |err| {
        log.err("vault_load_failed svix secret_ref={s} err={s}", .{ secret_ref, @errorName(err) });
        return .{ .secret = null };
    };
    return .{ .secret = secret };
}

/// Minimal extractor: parses `config_json.trigger.signature` and returns the
/// dup'd `secret_ref` string only. Avoids the full `parseSignature` scheme
/// construction for callers (svix) that don't need scheme metadata.
fn extractSecretRef(alloc: std.mem.Allocator, sig_json: []const u8) !?[]const u8 {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, sig_json, .{}) catch return null;
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const val = obj.get("secret_ref") orelse return null;
    const ref = switch (val) {
        .string => |s| s,
        else => return null,
    };
    if (ref.len == 0) return null;
    return try alloc.dupe(u8, ref);
}

const RowData = struct {
    workspace_id: []const u8,
    source: []const u8,
    token_raw: ?[]const u8,
    url_secret_ref: ?[]const u8,
    signature_json: ?[]const u8,
};

fn fetchZombieRow(conn: anytype, alloc: std.mem.Allocator, zombie_id: []const u8) !?RowData {
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text,
        \\       config_json->'trigger'->>'source',
        \\       config_json->'trigger'->>'token',
        \\       webhook_secret_ref,
        \\       config_json->'trigger'->'signature'
        \\FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id}));
    defer q.deinit();

    const row = try q.next() orelse return null;
    const ws = try row.get([]const u8, 0);
    const workspace_id = try alloc.dupe(u8, ws);
    errdefer alloc.free(workspace_id);
    const source = try alloc.dupe(u8, row.get([]const u8, 1) catch "");
    errdefer alloc.free(source);
    const token_raw = try dupeOptional(alloc, row.get([]const u8, 2) catch null);
    errdefer if (token_raw) |v| alloc.free(v);
    const url_secret_ref = try dupeOptional(alloc, row.get([]const u8, 3) catch null);
    errdefer if (url_secret_ref) |v| alloc.free(v);
    const signature_json = try dupeOptional(alloc, row.get([]const u8, 4) catch null);
    return RowData{
        .workspace_id = workspace_id,
        .source = source,
        .token_raw = token_raw,
        .url_secret_ref = url_secret_ref,
        .signature_json = signature_json,
    };
}

fn dupeOptional(alloc: std.mem.Allocator, v: ?[]const u8) !?[]const u8 {
    if (v) |s| return try alloc.dupe(u8, s);
    return null;
}

fn freeRowData(alloc: std.mem.Allocator, r: RowData) void {
    alloc.free(r.workspace_id);
    alloc.free(r.source);
    if (r.token_raw) |t| alloc.free(t);
    if (r.url_secret_ref) |s| alloc.free(s);
    if (r.signature_json) |j| alloc.free(j);
}

const ParsedSignature = struct {
    scheme: SignatureScheme,
    secret_ref: []const u8,
};

fn parseSignature(
    alloc: std.mem.Allocator,
    sig_json: []const u8,
    source: []const u8,
) !?ParsedSignature {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, sig_json, .{}) catch return null;
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };

    const secret_ref_val = obj.get("secret_ref") orelse return null;
    const secret_ref = switch (secret_ref_val) {
        .string => |s| s,
        else => return null,
    };
    if (secret_ref.len == 0) return null;

    const hit = webhook_verify.detectProvider(source, webhook_verify.NoHeaders{});
    const header_src = stringOrNull(obj.get("header")) orelse if (hit) |h| h.sig_header else return null;
    const prefix_src = stringOrNull(obj.get("prefix")) orelse if (hit) |h| h.prefix else "";
    const ts_src: ?[]const u8 = stringOrNull(obj.get("ts_header")) orelse if (hit) |h| h.ts_header else null;
    const version_src = if (hit) |h| h.hmac_version else "";
    const includes_ts = if (hit) |h| h.includes_timestamp else (ts_src != null);
    const drift = if (hit) |h| h.max_ts_drift_seconds else 300;

    const sig_header = try alloc.dupe(u8, header_src);
    errdefer alloc.free(sig_header);
    const prefix_dupe = try alloc.dupe(u8, prefix_src);
    errdefer alloc.free(prefix_dupe);
    const ts_dupe: ?[]const u8 = if (ts_src) |t| try alloc.dupe(u8, t) else null;
    errdefer if (ts_dupe) |t| alloc.free(t);
    const version_dupe = try alloc.dupe(u8, version_src);
    errdefer alloc.free(version_dupe);
    const secret_ref_dupe = try alloc.dupe(u8, secret_ref);

    return ParsedSignature{
        .scheme = .{
            .sig_header = sig_header,
            .prefix = prefix_dupe,
            .ts_header = ts_dupe,
            .hmac_version = version_dupe,
            .includes_timestamp = includes_ts,
            .max_ts_drift_seconds = drift,
        },
        .secret_ref = secret_ref_dupe,
    };
}

fn stringOrNull(v: ?std.json.Value) ?[]const u8 {
    const val = v orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

fn freeScheme(alloc: std.mem.Allocator, s: SignatureScheme) void {
    alloc.free(s.sig_header);
    alloc.free(s.prefix);
    if (s.ts_header) |t| alloc.free(t);
    alloc.free(s.hmac_version);
}
