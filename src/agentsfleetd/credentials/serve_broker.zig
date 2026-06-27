//! serve_broker.zig — the production assembly that turns the std-only broker
//! (`broker.zig`) into a live daemon singleton (M102 §3 serve wiring).
//!
//! The broker + integrations are pure/std-only + unit-testable because every
//! effect is injected via `integration.Deps`. This file builds the *real* effects
//! the daemon supplies at boot:
//!   * `HttpClientExchange` — the outbound HTTP boundary over `std.http.Client`
//!     (mirrors `fleet_bundle/github_net.zig`), used for the GitHub token exchange.
//!   * `loadGithubApp` — the option-1 platform-key load: read `(admin_ws,
//!     "github-app")` from the vault and dupe `{app_id, private_key_pem}` into a
//!     process-lifetime buffer. Degrades to `null` on any miss/parse error so the
//!     broker still serves `static` and a github mint returns `reconnect_required`.
//!   * `metricsSink` — a real observability hook logging each mint's outcome +
//!     latency (NEVER the token — VLT; `MintEvent` carries no secret).
//!
//! The RS256 signer (`auth/crypto/rs256_sign.signPemRs256`) is wired directly in
//! `cmd/serve.zig`; it needs no adapter (it is already the injectable `SignFn`).

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const integration = @import("integration.zig");
const vault = @import("../state/vault.zig");
const rs256_sign = @import("../auth/crypto/rs256_sign.zig");

const log = logging.scoped(.credential_broker);

/// Vault key_name holding the platform App secret under the admin workspace
/// (RULE UFS — the one spelling the load path uses). Adding an integration adds a
/// sibling key, never an env/schema growth (option 1).
pub const GITHUB_APP_VAULT_KEY: []const u8 = "github-app";

const FIELD_APP_ID: []const u8 = "app_id";
const FIELD_PRIVATE_KEY_PEM: []const u8 = "private_key_pem";
const FIELD_APP_SLUG: []const u8 = "app_slug";

/// Structured log message for an unconfigured/incomplete github platform key
/// (RULE UFS — one spelling across the vault-miss + missing-field sites).
const S_GITHUB_UNCONFIGURED: []const u8 = "credential_broker_github_unconfigured";

/// The fully-built broker dependencies + the process-lifetime secret bytes they
/// borrow. `serve.zig` holds this on its stack and `deinit`s it after the broker:
/// the broker copies `deps` by value, so the duped App key must outlive it.
pub const Built = struct {
    deps: integration.Deps,
    /// Owns the bytes `deps.platform.github` points at (null when unconfigured).
    github_app: ?integration.GithubApp,

    pub fn deinit(self: *Built, alloc: std.mem.Allocator) void {
        if (self.github_app) |a| {
            alloc.free(a.app_id);
            alloc.free(a.private_key_pem);
        }
    }
};

/// Build the production `integration.Deps` for the broker — the ONE place that
/// knows which integrations carry a platform key (RULE CFG: `serve.zig` stays
/// integration-agnostic; the next platform-keyed integration is added here, not
/// in the boot path). Loads each integration's platform secret from the admin
/// workspace vault (M102 ships `github`; `static` needs none), wires the injected
/// HTTP boundary + the RS256 signer + the metrics sink. Degrades closed: an unset
/// pointer or a vault miss leaves that integration null, never failing the boot.
/// `exchange` must outlive the broker (a stable pointer in the caller's frame).
pub fn buildDeps(alloc: std.mem.Allocator, pool: *pg.Pool, exchange: *HttpClientExchange, admin_ws_id: []const u8) Built {
    const github_app: ?integration.GithubApp = if (admin_ws_id.len == 0) blk: {
        log.info("credential_broker_static_only", .{ .reason = "platform_admin_workspace_id_unset" });
        break :blk null;
    } else blk: {
        const conn = pool.acquire() catch break :blk null;
        defer pool.release(conn);
        break :blk loadGithubApp(alloc, conn, admin_ws_id);
    };
    return .{
        .github_app = github_app,
        .deps = .{
            .platform = .{ .github = github_app },
            .http = exchange.exchange(),
            .sign = rs256_sign.signPemRs256,
            .metrics = metricsSink(),
        },
    };
}

/// The outbound HTTP boundary the broker performs token exchanges over. Holds the
/// daemon's blocking `Io`; a fresh `std.http.Client` per call (mints are rare —
/// only a cold-cache miss), mirroring `github_net.download`. No retained socket,
/// so no lifecycle to leak.
pub const HttpClientExchange = struct {
    io: std.Io,

    pub fn exchange(self: *HttpClientExchange) integration.HttpExchange {
        return .{ .ptr = self, .postFn = postImpl };
    }

    fn postImpl(ptr: *anyopaque, alloc: std.mem.Allocator, req: integration.HttpRequest) anyerror!integration.HttpResponse {
        const self: *HttpClientExchange = @ptrCast(@alignCast(ptr));
        var client: std.http.Client = .{ .allocator = alloc, .io = self.io };
        defer client.deinit();

        const auth = try std.fmt.allocPrint(alloc, "Bearer {s}", .{req.bearer});
        defer alloc.free(auth);

        // BUFFER GATE: ArrayList(u8) for the response body — fetch streams into it.
        var body: std.ArrayList(u8) = .empty;
        var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

        var headers: [4]std.http.Header = undefined;
        var n: usize = 0;
        headers[n] = .{ .name = "authorization", .value = auth };
        n += 1;
        headers[n] = .{ .name = "accept", .value = req.accept };
        n += 1;
        headers[n] = .{ .name = "user-agent", .value = req.user_agent };
        n += 1;
        if (req.body.len > 0) {
            headers[n] = .{ .name = "content-type", .value = "application/json" };
            n += 1;
        }

        const result = client.fetch(.{
            .location = .{ .url = req.url },
            .method = .POST,
            .payload = if (req.body.len > 0) req.body else null,
            .extra_headers = headers[0..n],
            .response_writer = &aw.writer,
        }) catch return error.HttpExchangeFailed;

        return .{ .status = @intFromEnum(result.status), .body = aw.toOwnedSlice() catch return error.OutOfMemory };
    }
};

/// Load the platform GitHub App key from `(admin_ws_id, "github-app")` in the
/// vault (option 1). Returns the App id + private key PEM duped into `alloc`
/// (process-lifetime — the broker signs JWTs from it on every cold mint; the
/// caller frees both at shutdown). Degrades to `null` on ANY miss — pointer row
/// absent, malformed JSON, or a missing field — so an unconfigured platform still
/// boots (broker serves `static`; github mints return `reconnect_required`). The
/// PEM is platform-side only and never logged (VLT).
pub fn loadGithubApp(alloc: std.mem.Allocator, conn: *pg.Conn, admin_ws_id: []const u8) ?integration.GithubApp {
    var parsed = vault.loadJson(alloc, conn, admin_ws_id, GITHUB_APP_VAULT_KEY) catch {
        log.info(S_GITHUB_UNCONFIGURED, .{ .reason = "vault_miss" });
        return null;
    };
    defer parsed.deinit();

    const obj = parsed.value.object;
    const app_id_v = obj.get(FIELD_APP_ID) orelse return logMissing("app_id");
    const pem_v = obj.get(FIELD_PRIVATE_KEY_PEM) orelse return logMissing("private_key_pem");
    if (app_id_v != .string or pem_v != .string) return logMissing("non_string_field");

    const app_id = alloc.dupe(u8, app_id_v.string) catch return null;
    const pem = alloc.dupe(u8, pem_v.string) catch {
        alloc.free(app_id);
        return null;
    };
    log.info("credential_broker_github_configured", .{ .app_id = app_id });
    return .{ .app_id = app_id, .private_key_pem = pem };
}

/// Load the platform GitHub App slug from `(admin_ws_id, "github-app")` for the
/// connect install URL (`github.com/apps/{slug}/installations/new`). Duped into
/// `alloc` (process-lifetime; the caller frees at shutdown). Null on any miss so
/// the connect route degrades closed rather than minting a dead install URL.
pub fn loadGithubAppSlug(alloc: std.mem.Allocator, pool: *pg.Pool, admin_ws_id: []const u8) ?[]const u8 {
    if (admin_ws_id.len == 0) return null;
    const conn = pool.acquire() catch return null;
    defer pool.release(conn);
    var parsed = vault.loadJson(alloc, conn, admin_ws_id, GITHUB_APP_VAULT_KEY) catch return null;
    defer parsed.deinit();
    const slug_v = parsed.value.object.get(FIELD_APP_SLUG) orelse return null;
    if (slug_v != .string) return null;
    return alloc.dupe(u8, slug_v.string) catch null;
}

fn logMissing(field: []const u8) ?integration.GithubApp {
    log.info(S_GITHUB_UNCONFIGURED, .{ .reason = "missing_field", .field = field });
    return null;
}

/// A real metrics hook: log each mint's integration + outcome + latency + cache
/// hit. `MintEvent` carries no token (VLT), so this can never leak a secret.
pub fn metricsSink() integration.Metrics {
    // SAFETY: onMint is stateless (logs only) and never dereferences `ptr` — the
    // hook carries no context, mirroring `integration.nullDeps`'s ignoreMint.
    return .{ .ptr = undefined, .onMintFn = onMint };
}

fn onMint(_: *anyopaque, ev: integration.MintEvent) void {
    log.info("credential_mint", .{
        .integration = ev.integration,
        .outcome = ev.outcome,
        .latency_ms = ev.latency_ms,
        .cache_hit = ev.cache_hit,
    });
}

// ── Tests ────────────────────────────────────────────────────────────────────
const testing = std.testing;

test "metricsSink emits without dereferencing its opaque ptr" {
    const sink = metricsSink();
    // ptr is undefined by contract; onMint must never touch it.
    sink.onMint(.{ .integration = "github", .outcome = "ok", .latency_ms = 12, .cache_hit = false });
}

test "exchange wires a post boundary over the client" {
    var ex = HttpClientExchange{ .io = @import("common").globalIo() };
    const boundary = ex.exchange();
    // The boundary points back at the exchange struct (no network here).
    try testing.expect(boundary.ptr == @as(*anyopaque, @ptrCast(&ex)));
}
