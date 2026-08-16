//! Webhook-sig lookup: resolves Bearer token + per-fleet HMAC scheme/secret
//! for the webhook_sig middleware. Lives in `src/agentsfleetd/cmd/` so it can
//! import both auth middleware and the fleet runtime registry.
//!
//! Secret resolution: each fleet declares one or more `triggers[].source`
//! entries (e.g. `github`). Each names an HMAC scheme and a workspace
//! credential. The credential is stored at vault key `<source>`
//! (overridable via `triggers[].credential_name`) and decodes to a JSON
//! object whose `webhook_secret` field is the HMAC key.
//!
//! Multi-webhook-per-fleet URL routing (`{source}` segment in the webhook
//! URL) lands with the install + list response slice. Until then the URL
//! carries `fleet_id` alone and the queries below pull the first webhook
//! trigger from the `triggers[]` array.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const crypto_store = @import("../secrets/crypto_store.zig");
const vault = @import("../state/vault.zig");
const webhook_verify = @import("../fleet_runtime/webhook_verify.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");

const LookupResult = auth_mw.webhook_sig_mod.LookupResult;
const SignatureScheme = auth_mw.webhook_sig_mod.SignatureScheme;
const SvixLookupResult = auth_mw.svix_signature_mod.SvixLookupResult;

const log = logging.scoped(.webhook_sig_lookup);

const WEBHOOK_SECRET_FIELD = "webhook_secret";

pub fn lookup(
    pool: *pg.Pool,
    fleet_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?LookupResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const row_data = (try fetchHmacRow(conn, alloc, fleet_id)) orelse return null;
    defer freeHmacRow(alloc, row_data);

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
            signature_secret = loadWebhookSecret(alloc, conn, row_data.workspace_id, credential_name);
        }
    }

    return .{
        .signature_scheme = scheme,
        .signature_secret = signature_secret,
    };
}

/// Svix middleware lookup. Fetches the Clerk-style `signature.secret_ref` from
/// the fleet's config_json and resolves it to the `whsec_<base64>` secret via
/// the workspace vault. Middleware handles prefix stripping + base64 decoding.
pub fn lookupSvix(
    pool: *pg.Pool,
    fleet_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?SvixLookupResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const row_data = (try fetchSvixRow(conn, alloc, fleet_id)) orelse return null;
    defer freeSvixRow(alloc, row_data);

    const sig_json = row_data.signature_json orelse return .{ .secret = null };
    const secret_ref = (try extractSecretRef(alloc, sig_json)) orelse return .{ .secret = null };
    defer alloc.free(secret_ref);

    const secret = crypto_store.load(alloc, conn, row_data.workspace_id, secret_ref) catch |err| {
        log.err("svix_vault_load_failed", .{ .error_code = error_codes.ERR_SECRET_NOT_FOUND, .secret_ref = secret_ref, .err = @errorName(err) });
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

const HmacRow = struct {
    workspace_id: []const u8,
    source: []const u8,
    credential_name_override: ?[]const u8,
};

const SvixRow = struct {
    workspace_id: []const u8,
    signature_json: ?[]const u8,
};

fn fetchHmacRow(conn: anytype, alloc: std.mem.Allocator, fleet_id: []const u8) !?HmacRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT z.workspace_id::text,
        \\       (SELECT trig->>'source'
        \\          FROM jsonb_array_elements(z.config_json->'x-agentsfleet'->'triggers') trig
        \\          WHERE trig->>'type' = 'webhook'
        \\          LIMIT 1),
        \\       (SELECT trig->>'credential_name'
        \\          FROM jsonb_array_elements(z.config_json->'x-agentsfleet'->'triggers') trig
        \\          WHERE trig->>'type' = 'webhook'
        \\          LIMIT 1)
        \\FROM core.fleets z WHERE z.id = $1::uuid
    , .{fleet_id}));
    defer q.deinit();

    const row = try q.next() orelse return null;
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const source = try alloc.dupe(u8, row.get([]const u8, 1) catch "");
    errdefer alloc.free(source);
    const credential_name_override = try dupeOptional(alloc, row.get([]const u8, 2) catch null);
    return HmacRow{
        .workspace_id = workspace_id,
        .source = source,
        .credential_name_override = credential_name_override,
    };
}

fn fetchSvixRow(conn: anytype, alloc: std.mem.Allocator, fleet_id: []const u8) !?SvixRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT z.workspace_id::text,
        \\       (SELECT trig->'signature'
        \\          FROM jsonb_array_elements(z.config_json->'x-agentsfleet'->'triggers') trig
        \\          WHERE trig->>'type' = 'webhook'
        \\          LIMIT 1)
        \\FROM core.fleets z WHERE z.id = $1::uuid
    , .{fleet_id}));
    defer q.deinit();

    const row = try q.next() orelse return null;
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const signature_json = try dupeOptional(alloc, row.get([]const u8, 1) catch null);
    return SvixRow{
        .workspace_id = workspace_id,
        .signature_json = signature_json,
    };
}

fn dupeOptional(alloc: std.mem.Allocator, v: ?[]const u8) !?[]const u8 {
    if (v) |s| return try alloc.dupe(u8, s);
    return null;
}

fn freeHmacRow(alloc: std.mem.Allocator, r: HmacRow) void {
    alloc.free(r.workspace_id);
    alloc.free(r.source);
    if (r.credential_name_override) |s| alloc.free(s);
}

fn freeSvixRow(alloc: std.mem.Allocator, r: SvixRow) void {
    alloc.free(r.workspace_id);
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
        log.warn("webhook_credential_load_failed", .{ .error_code = error_codes.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, .workspace_id = workspace_id, .key = key_name, .err = @errorName(err) });
        return null;
    };
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const val = obj.get(WEBHOOK_SECRET_FIELD) orelse {
        log.warn("webhook_credential_missing_field", .{ .error_code = error_codes.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, .workspace_id = workspace_id, .key = key_name });
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

test "extractSecretRef refuses every shape that names no usable ref" {
    const alloc = std.testing.allocator;
    // Each of these reaches the middleware as "no Svix secret configured",
    // which must fail closed rather than resolve to something arbitrary.
    const refused = [_][]const u8{
        "not json at all",
        "[\"secret_ref\"]",
        "{\"other\":\"x\"}",
        "{\"secret_ref\":42}",
        "{\"secret_ref\":\"\"}",
    };
    for (refused) |sig_json| {
        try std.testing.expect(try extractSecretRef(alloc, sig_json) == null);
    }
}

test "extractSecretRef returns the ref as an owned copy" {
    const alloc = std.testing.allocator;
    const ref = (try extractSecretRef(alloc, "{\"secret_ref\":\"whsec_key\"}")).?;
    defer alloc.free(ref);
    try std.testing.expectEqualStrings("whsec_key", ref);
}

const TEST_CONFIG = webhook_verify.VerifyConfig{
    .name = "github",
    .sig_header = "X-Hub-Signature-256",
    .ts_header = "X-Hub-Timestamp",
    .prefix = "sha256=",
    .hmac_version = "v1",
    .includes_timestamp = true,
    .max_ts_drift_seconds = 300,
};

test "schemeFromConfig copies every field and freeScheme releases all of them" {
    const alloc = std.testing.allocator;
    const scheme = try schemeFromConfig(alloc, TEST_CONFIG);
    defer freeScheme(alloc, scheme);

    // Copies, not borrows: the config outlives no request, so a borrowed
    // header name would dangle by the time the middleware verifies a payload.
    try std.testing.expectEqualStrings(TEST_CONFIG.sig_header, scheme.sig_header);
    try std.testing.expectEqualStrings(TEST_CONFIG.prefix, scheme.prefix);
    try std.testing.expectEqualStrings(TEST_CONFIG.ts_header.?, scheme.ts_header.?);
    try std.testing.expectEqualStrings(TEST_CONFIG.hmac_version, scheme.hmac_version);
    try std.testing.expect(scheme.includes_timestamp);
    try std.testing.expectEqual(TEST_CONFIG.max_ts_drift_seconds, scheme.max_ts_drift_seconds);
}

test "schemeFromConfig unwinds every partial copy when an allocation fails" {
    // One run per allocation the function makes, so each errdefer arm unwinds
    // in turn. testing.allocator underneath fails the test on any leak.
    for (0..4) |fail_index| {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index });
        try std.testing.expectError(error.OutOfMemory, schemeFromConfig(failing.allocator(), TEST_CONFIG));
    }
}

test "freeSvixRow releases the row with and without a signature payload" {
    const alloc = std.testing.allocator;
    freeSvixRow(alloc, .{
        .workspace_id = try alloc.dupe(u8, "ws-with-signature"),
        .signature_json = try alloc.dupe(u8, "{\"secret_ref\":\"whsec_key\"}"),
    });
    freeSvixRow(alloc, .{
        .workspace_id = try alloc.dupe(u8, "ws-without-signature"),
        .signature_json = null,
    });
}
