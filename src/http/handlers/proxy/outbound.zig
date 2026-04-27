//! Outbound proxy — grant check → firewall → credential inject → HTTP proxy → echo strip.
//! Called by execute.zig after authentication. Reuses M6 firewall policy engine.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const firewall = @import("../../../zombie/firewall/firewall.zig");
const crypto_store = @import("../../../secrets/crypto_store.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");

const log = std.log.scoped(.outbound_proxy);

pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024; // 10 MB cap

const PipelineInput = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    target: []const u8,   // "slack.com/api/chat.postMessage"
    method: []const u8,
    body: ?[]const u8,
    credential_ref: []const u8,
};

const PipelineResult = struct {
    status: u16,
    body: []const u8,        // owned by caller's allocator
    action_id: []const u8,   // UUIDv7 string
    firewall_decision: []const u8,
    credential_injected: bool,
    truncated: bool,
};

pub const PipelineError = error{
    DomainBlocked,
    InjectionDetected,
    ApprovalRequired,
    GrantNotFound,
    GrantPending,
    GrantDenied,
    CredentialNotFound,
    TargetError,
    OutOfMemory,
};

// ── Service → domain mapping ───────────────────────────────────────────────

const ServiceEntry = struct { domain: []const u8, service: []const u8 };

const SERVICE_MAP = [_]ServiceEntry{
    .{ .domain = "slack.com",             .service = "slack" },
    .{ .domain = "hooks.slack.com",       .service = "slack" },
    .{ .domain = "gmail.googleapis.com",  .service = "gmail" },
    .{ .domain = "www.googleapis.com",    .service = "gmail" },
    .{ .domain = "api.agentmail.to",      .service = "agentmail" },
    .{ .domain = "discord.com",           .service = "discord" },
    .{ .domain = "discordapp.com",        .service = "discord" },
    .{ .domain = "grafana.com",           .service = "grafana" },
};

pub fn serviceForDomain(domain: []const u8) ?[]const u8 {
    for (SERVICE_MAP) |entry| {
        if (std.mem.eql(u8, domain, entry.domain)) return entry.service;
    }
    return null;
}

// ── Domain extraction ──────────────────────────────────────────────────────

/// Extract domain from "domain/path" or "https://domain/path".
/// Caller does not own returned slice — points into `target`.
pub fn extractDomain(target: []const u8) []const u8 {
    var rest = target;
    if (std.mem.startsWith(u8, rest, "https://")) rest = rest["https://".len..];
    if (std.mem.startsWith(u8, rest, "http://"))  rest = rest["http://".len..];
    const slash = std.mem.indexOfScalar(u8, rest, '/') orelse rest.len;
    return rest[0..slash];
}

/// Extract the path component from a target for firewall rule matching.
/// Accounts for an optional scheme prefix (https://, http://) and the domain.
/// Always returns a slice starting with '/', or "/" when there is no path.
/// Caller does not own returned slice — points into `target`.
pub fn extractPath(target: []const u8, domain: []const u8) []const u8 {
    var scheme_len: usize = 0;
    if (std.mem.startsWith(u8, target, "https://")) scheme_len = "https://".len
    else if (std.mem.startsWith(u8, target, "http://")) scheme_len = "http://".len;
    const path_start = scheme_len + domain.len;
    return if (path_start < target.len) target[path_start..] else "/";
}

// ── Grant check ───────────────────────────────────────────────────────────

const GrantStatus = enum { approved, pending, denied, not_found };

fn checkGrant(conn: *pg.Conn, zombie_id: []const u8, service: []const u8) GrantStatus {
    var q = PgQuery.from(conn.query(
        \\SELECT status FROM core.integration_grants
        \\WHERE zombie_id = $1::uuid AND service = $2
        \\LIMIT 1
    , .{ zombie_id, service }) catch return .not_found);
    defer q.deinit();

    const row_opt = q.next() catch return .not_found;
    const row = row_opt orelse return .not_found;
    const status_str = row.get([]u8, 0) catch return .not_found;
    if (std.mem.eql(u8, status_str, "approved")) return .approved;
    if (std.mem.eql(u8, status_str, "pending"))  return .pending;
    return .denied;
}

// ── HTTP method parsing ────────────────────────────────────────────────────

fn parseMethod(method_str: []const u8) std.http.Method {
    if (std.ascii.eqlIgnoreCase(method_str, "GET"))    return .GET;
    if (std.ascii.eqlIgnoreCase(method_str, "POST"))   return .POST;
    if (std.ascii.eqlIgnoreCase(method_str, "PUT"))    return .PUT;
    if (std.ascii.eqlIgnoreCase(method_str, "PATCH"))  return .PATCH;
    if (std.ascii.eqlIgnoreCase(method_str, "DELETE")) return .DELETE;
    return .GET;
}

// ── Outbound HTTP call ────────────────────────────────────────────────────

const ProxyResult = struct {
    status: u16,
    body: []u8,       // owned by alloc
    truncated: bool,
};

fn proxyCall(
    alloc: std.mem.Allocator,
    target: []const u8,
    method: []const u8,
    credential: []const u8,
    body: ?[]const u8,
) !ProxyResult {
    const scheme = if (std.mem.startsWith(u8, target, "http://")) "" else "https://";
    const url = if (std.mem.startsWith(u8, target, "http"))
        try alloc.dupe(u8, target)
    else
        try std.fmt.allocPrint(alloc, "{s}{s}", .{ scheme, target });
    defer alloc.free(url);

    var client = std.http.Client{ .allocator = alloc };
    defer client.deinit();

    var response_buf: std.ArrayList(u8) = .{};
    defer response_buf.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &response_buf);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{credential});
    defer alloc.free(auth_header);

    const uri = std.Uri.parse(url) catch return error.TargetError;
    const result = client.fetch(.{
        .method = parseMethod(method),
        .location = .{ .uri = uri },
        .extra_headers = &.{
            .{ .name = "Authorization", .value = auth_header },
        },
        .payload = body,
        .response_writer = &aw.writer,
    }) catch {
        log.err("outbound_proxy.proxy_fail target={s}", .{target});
        return error.TargetError;
    };

    const truncated = response_buf.items.len > MAX_RESPONSE_BYTES;
    const final_body = if (truncated)
        try alloc.dupe(u8, response_buf.items[0..MAX_RESPONSE_BYTES])
    else
        try alloc.dupe(u8, response_buf.items);

    return .{
        .status = @intFromEnum(result.status),
        .body = final_body,
        .truncated = truncated,
    };
}

// ── Credential echo strip ─────────────────────────────────────────────────

pub fn stripEcho(alloc: std.mem.Allocator, body: []const u8, credential: []const u8) ![]u8 {
    // Empty credential means nothing to strip — return body as-is.
    if (credential.len == 0) return alloc.dupe(u8, body);
    const fw = firewall.Firewall.init(&.{}, &.{});
    const scan = fw.scanResponseBody(body, &.{credential});
    _ = scan; // scan result logged upstream; strip regardless of classification
    // Simple redaction: replace all occurrences of the credential with [REDACTED]
    const count = std.mem.count(u8, body, credential);
    if (count == 0) return alloc.dupe(u8, body);
    const redacted = "[REDACTED]";
    const new_len = body.len - (count * credential.len) + (count * redacted.len);
    var out = try alloc.alloc(u8, new_len);
    var src = body;
    var dst: usize = 0;
    while (std.mem.indexOf(u8, src, credential)) |pos| {
        @memcpy(out[dst..][0..pos], src[0..pos]);
        dst += pos;
        @memcpy(out[dst..][0..redacted.len], redacted);
        dst += redacted.len;
        src = src[pos + credential.len ..];
    }
    @memcpy(out[dst..], src);
    return out;
}

// ── Main pipeline ─────────────────────────────────────────────────────────

pub fn run(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    input: PipelineInput,
) PipelineError!PipelineResult {
    const domain = extractDomain(input.target);
    const service = serviceForDomain(domain) orelse {
        log.warn("outbound_proxy.domain_blocked zombie_id={s} domain={s}", .{
            input.zombie_id, domain,
        });
        return PipelineError.DomainBlocked;
    };

    // Grant check
    switch (checkGrant(conn, input.zombie_id, service)) {
        .approved  => {},
        .pending   => return PipelineError.GrantPending,
        .denied    => return PipelineError.GrantDenied,
        .not_found => return PipelineError.GrantNotFound,
    }

    // Firewall: injection scan on request body.
    const fw_path = extractPath(input.target, domain);

    if (input.body) |b| {
        const fw = firewall.Firewall.init(&.{}, &.{});
        const fw_req = firewall.OutboundRequest{
            .tool = "execute",
            .method = input.method,
            .domain = domain,
            .path = fw_path,
            .body = b,
        };
        switch (fw.inspectRequest(fw_req)) {
            .block => return PipelineError.InjectionDetected,
            .requires_approval => return PipelineError.ApprovalRequired,
            .allow => {},
        }
    }

    // Credential fetch
    const credential = crypto_store.load(alloc, conn, input.workspace_id, input.credential_ref) catch {
        log.err("outbound_proxy.cred_not_found workspace_id={s} ref={s}", .{
            input.workspace_id, input.credential_ref,
        });
        return PipelineError.CredentialNotFound;
    };
    defer alloc.free(credential);

    // Proxy outbound call
    const proxy = proxyCall(alloc, input.target, input.method, credential, input.body) catch
        return PipelineError.TargetError;
    defer alloc.free(proxy.body);

    // Strip credential echo from response
    const clean_body = stripEcho(alloc, proxy.body, credential) catch
        return PipelineError.OutOfMemory;

    const action_id = id_format.generateZombieId(alloc) catch
        return PipelineError.OutOfMemory;

    log.info("outbound_proxy.ok zombie_id={s} service={s} status={d} action_id={s}", .{
        input.zombie_id, service, proxy.status, action_id,
    });

    return .{
        .status = proxy.status,
        .body = clean_body,
        .action_id = action_id,
        .firewall_decision = "allow",
        .credential_injected = true,
        .truncated = proxy.truncated,
    };
}
