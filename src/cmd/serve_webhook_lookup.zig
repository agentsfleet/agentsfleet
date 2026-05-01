//! Webhook-sig lookup: resolves Bearer token + per-zombie HMAC scheme/secret
//! for the webhook_sig middleware. Lives in `src/cmd/` so it can import both
//! `src/auth/` and `src/zombie/`.
//!
//! Secret resolution: each zombie declares a `trigger.source` (e.g. `github`)
//! that names the HMAC scheme and the workspace credential to read. The
//! credential is stored at vault key `zombie:<source>` (overridable via
//! `trigger.credential_name`) and decodes to a JSON object whose
//! `webhook_secret` field is the HMAC key.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const crypto_store = @import("../secrets/crypto_store.zig");
const vault = @import("../state/vault.zig");
const credential_key = @import("../zombie/credential_key.zig");
const webhook_verify = @import("../zombie/webhook_verify.zig");
const auth_mw = @import("../auth/middleware/mod.zig");

const LookupResult = auth_mw.webhook_sig_mod.LookupResult;
const SignatureScheme = auth_mw.webhook_sig_mod.SignatureScheme;
const SvixLookupResult = auth_mw.svix_signature_mod.SvixLookupResult;

const log = std.log.scoped(.webhook_sig_lookup);

const WEBHOOK_SECRET_FIELD = "webhook_secret";

pub fn lookup(
    pool: *pg.Pool,
    zombie_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?LookupResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const row_data = (try fetchZombieRow(conn, alloc, zombie_id)) orelse return null;
    defer freeRowData(alloc, row_data);

    var scheme: ?SignatureScheme = null;
    var signature_secret: ?[]const u8 = null;
    errdefer if (scheme) |s| freeScheme(alloc, s);
    errdefer if (signature_secret) |s| alloc.free(s);

    if (row_data.source.len > 0) {
        if (webhook_verify.detectProvider(row_data.source, webhook_verify.NoHeaders{})) |cfg| {
            // Always populate the scheme when the provider is recognized, so
            // the middleware fails closed with UZ-WH-020 on a missing vault
            // credential (RFC: never silently degrade auth on misconfig).
            scheme = try schemeFromConfig(alloc, cfg);
            const credential_name = row_data.credential_name_override orelse row_data.source;
            const key_name = try credential_key.allocKeyName(alloc, credential_name);
            defer alloc.free(key_name);
            signature_secret = loadWebhookSecret(alloc, conn, row_data.workspace_id, key_name);
        }
    }

    return .{
        .signature_scheme = scheme,
        .signature_secret = signature_secret,
    };
}

/// Svix middleware lookup. Fetches the Clerk-style `signature.secret_ref` from
/// the zombie's config_json and resolves it to the `whsec_<base64>` secret via
/// the workspace vault. Middleware handles prefix stripping + base64 decoding.
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
    credential_name_override: ?[]const u8,
    signature_json: ?[]const u8,
};

fn fetchZombieRow(conn: anytype, alloc: std.mem.Allocator, zombie_id: []const u8) !?RowData {
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text,
        \\       config_json->'x-usezombie'->'trigger'->>'source',
        \\       config_json->'x-usezombie'->'trigger'->>'credential_name',
        \\       config_json->'x-usezombie'->'trigger'->'signature'
        \\FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id}));
    defer q.deinit();

    const row = try q.next() orelse return null;
    const ws = try row.get([]const u8, 0);
    const workspace_id = try alloc.dupe(u8, ws);
    errdefer alloc.free(workspace_id);
    const source = try alloc.dupe(u8, row.get([]const u8, 1) catch "");
    errdefer alloc.free(source);
    const credential_name_override = try dupeOptional(alloc, row.get([]const u8, 2) catch null);
    errdefer if (credential_name_override) |v| alloc.free(v);
    const signature_json = try dupeOptional(alloc, row.get([]const u8, 3) catch null);
    return RowData{
        .workspace_id = workspace_id,
        .source = source,
        .credential_name_override = credential_name_override,
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
    if (r.credential_name_override) |s| alloc.free(s);
    if (r.signature_json) |j| alloc.free(j);
}

/// Load the workspace credential at `key_name`, parse it, and return the
/// `webhook_secret` field as an owned slice. Returns null on any failure
/// (credential missing, malformed JSON, missing field) — the middleware
/// fails closed downstream.
fn loadWebhookSecret(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ?[]const u8 {
    var parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |err| {
        log.warn("webhook_credential_load_failed workspace_id={s} key={s} err={s}", .{ workspace_id, key_name, @errorName(err) });
        return null;
    };
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const val = obj.get(WEBHOOK_SECRET_FIELD) orelse {
        log.warn("webhook_credential_missing_field workspace_id={s} key={s}", .{ workspace_id, key_name });
        return null;
    };
    const secret = switch (val) {
        .string => |s| s,
        else => return null,
    };
    if (secret.len == 0) return null;
    return alloc.dupe(u8, secret) catch null;
}

fn schemeFromConfig(alloc: std.mem.Allocator, cfg: webhook_verify.VerifyConfig) !SignatureScheme {
    const sig_header = try alloc.dupe(u8, cfg.sig_header);
    errdefer alloc.free(sig_header);
    const prefix = try alloc.dupe(u8, cfg.prefix);
    errdefer alloc.free(prefix);
    const ts_header: ?[]const u8 = if (cfg.ts_header) |t| try alloc.dupe(u8, t) else null;
    errdefer if (ts_header) |t| alloc.free(t);
    const hmac_version = try alloc.dupe(u8, cfg.hmac_version);
    return .{
        .sig_header = sig_header,
        .prefix = prefix,
        .ts_header = ts_header,
        .hmac_version = hmac_version,
        .includes_timestamp = cfg.includes_timestamp,
        .max_ts_drift_seconds = cfg.max_ts_drift_seconds,
    };
}

fn freeScheme(alloc: std.mem.Allocator, s: SignatureScheme) void {
    alloc.free(s.sig_header);
    alloc.free(s.prefix);
    if (s.ts_header) |t| alloc.free(t);
    alloc.free(s.hmac_version);
}
