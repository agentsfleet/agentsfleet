//! serve_broker.zig — the production assembly that turns the std-only broker
//! (`broker.zig`) into a live daemon singleton (M102 §3 serve wiring).
//!
//! The broker + integrations are pure/std-only + unit-testable because every
//! effect is injected via `integration.Deps`. This file builds the *real* effects
//! the daemon supplies at boot:
//!   * `HttpClientExchange` — the outbound HTTP boundary over `std.http.Client`
//!     (mirrors `fleet_library/github_net.zig`), used for the GitHub token exchange.
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
const common = @import("common");
const logging = @import("log");
const call_deadline = @import("call_deadline");
const http_pin = @import("http_pin");

const integration = @import("integration.zig");
const vault = @import("../state/vault.zig");
const rs256_sign = @import("../auth/crypto/rs256_sign.zig");

const log = logging.scoped(.credential_broker);

/// Owner-safe deadline for the outbound token exchange (the broker maps any
/// failure to `mint_failed{transient}`, so it needs the bound, not the taxonomy).
const Scheduler = call_deadline.ProcessScheduler;

/// Deadline for a broker token exchange (installation-token mint or oauth2
/// refresh) — a rare cold-cache vendor round-trip. Mirrors the connector layer's
/// `bounded_fetch.TOKEN_EXCHANGE_DEADLINE_MS`; a hung vendor token endpoint must
/// never stall the broker (fail closed → transient, re-minted on the next call).
const MINT_DEADLINE_MS: u31 = 10_000;

/// Vault key_name holding the platform App secret under the admin workspace
/// (RULE UFS — the one spelling the load path uses). Adding an integration adds a
/// sibling key, never an env/schema growth (option 1).
pub const GITHUB_APP_VAULT_KEY: []const u8 = "github-app";

/// The `<provider>-app` stem, comptime-joined to each provider id so the key is
/// derived from the single `common.PROVIDER_*` source, never re-spelled (RULE UFS;
/// mirrors `connectors/oauth2.zig` `APP_VAULT_KEY_SUFFIX`). Bare literal so `++`
/// yields an array — a `[]const u8` slice cannot be concatenated at comptime.
const APP_KEY_SUFFIX = "-app";
const ZOHO_APP_VAULT_KEY = common.PROVIDER_ZOHO ++ APP_KEY_SUFFIX;
const JIRA_APP_VAULT_KEY = common.PROVIDER_JIRA ++ APP_KEY_SUFFIX;
const LINEAR_APP_VAULT_KEY = common.PROVIDER_LINEAR ++ APP_KEY_SUFFIX;

const FIELD_APP_ID: []const u8 = "app_id";
const FIELD_PRIVATE_KEY_PEM: []const u8 = "private_key_pem";
const FIELD_APP_SLUG: []const u8 = "app_slug";
const FIELD_CLIENT_ID: []const u8 = "client_id";
const FIELD_CLIENT_SECRET: []const u8 = "client_secret";

/// Structured log message for an unconfigured/incomplete github platform key
/// (RULE UFS — one spelling across the vault-miss + missing-field sites).
const S_GITHUB_UNCONFIGURED: []const u8 = "credential_broker_github_unconfigured";
/// Structured log message for an unconfigured/incomplete oauth2 platform app.
const S_OAUTH_UNCONFIGURED: []const u8 = "credential_broker_oauth_unconfigured";
/// Log message for the static-only fallback (no admin workspace / no conn).
const S_STATIC_ONLY: []const u8 = "credential_broker_static_only";
/// `reason=`/`field=` labels shared across the github + oauth load paths (UFS).
const R_VAULT_MISS: []const u8 = "vault_miss";
const R_MISSING_FIELD: []const u8 = "missing_field";
const F_NON_STRING: []const u8 = "non_string_field";

/// The fully-built broker dependencies + the process-lifetime secret bytes they
/// borrow. `serve.zig` holds this on its stack and `deinit`s it after the broker:
/// the broker copies `deps` by value, so the duped App key must outlive it.
pub const Built = struct {
    deps: integration.Deps,
    /// Owns the bytes `deps.platform.<provider>` points at (null when that
    /// provider's app is unconfigured). The broker copies `deps` by value, so
    /// these owned secrets must outlive it — freed at shutdown.
    github_app: ?integration.GithubApp,
    zoho_app: ?integration.OauthApp = null,
    jira_app: ?integration.OauthApp = null,
    linear_app: ?integration.OauthApp = null,

    pub fn deinit(self: *Built, alloc: std.mem.Allocator) void {
        if (self.github_app) |a| {
            alloc.free(a.app_id);
            alloc.free(a.private_key_pem);
            if (a.app_slug) |s| alloc.free(s);
        }
        freeOauthApp(alloc, self.zoho_app);
        freeOauthApp(alloc, self.jira_app);
        freeOauthApp(alloc, self.linear_app);
    }
};

fn freeOauthApp(alloc: std.mem.Allocator, app: ?integration.OauthApp) void {
    if (app) |a| {
        alloc.free(a.client_id);
        alloc.free(a.client_secret);
    }
}

/// Build the production `integration.Deps` for the broker — the ONE place that
/// knows which integrations carry a platform key (RULE CFG: `serve.zig` stays
/// integration-agnostic; the next platform-keyed integration is added here, not
/// in the boot path). Loads each integration's platform secret from the admin
/// workspace vault (M102 ships `github`; `static` needs none), wires the injected
/// HTTP boundary + the RS256 signer + the metrics sink. Degrades closed: an unset
/// pointer or a vault miss leaves that integration null, never failing the boot.
/// `exchange` must outlive the broker (a stable pointer in the caller's frame).
pub fn buildDeps(alloc: std.mem.Allocator, pool: *pg.Pool, exchange: *HttpClientExchange, admin_ws_id: []const u8) Built {
    const secrets = loadPlatformSecrets(alloc, pool, admin_ws_id);
    return .{
        .github_app = secrets.github,
        .zoho_app = secrets.zoho,
        .jira_app = secrets.jira,
        .linear_app = secrets.linear,
        .deps = .{
            .platform = secrets,
            .http = exchange.exchange(),
            .sign = rs256_sign.signPemRs256,
            .metrics = metricsSink(),
        },
    };
}

/// Read every platform-keyed integration's secret from the admin-workspace vault
/// on one connection: `github-app` (App key) + `zoho-app`/`jira-app`/`linear-app`
/// (OAuth client id/secret for the refresh mints). Each field degrades to `null`
/// independently on a miss/parse error, so an unconfigured provider leaves the
/// others live and the broker still boots. An unset admin workspace or a pool
/// acquire failure yields all-null (static-only) — never a boot failure.
fn loadPlatformSecrets(alloc: std.mem.Allocator, pool: *pg.Pool, admin_ws_id: []const u8) integration.PlatformSecrets {
    if (admin_ws_id.len == 0) {
        log.info(S_STATIC_ONLY, .{ .reason = "platform_admin_workspace_id_unset" });
        return .{};
    }
    const conn = pool.acquire() catch {
        log.info(S_STATIC_ONLY, .{ .reason = "pool_acquire_failed" });
        return .{};
    };
    defer pool.release(conn);
    return .{
        .github = loadGithubApp(alloc, conn, admin_ws_id),
        .zoho = loadOauthApp(alloc, conn, admin_ws_id, ZOHO_APP_VAULT_KEY),
        .jira = loadOauthApp(alloc, conn, admin_ws_id, JIRA_APP_VAULT_KEY),
        .linear = loadOauthApp(alloc, conn, admin_ws_id, LINEAR_APP_VAULT_KEY),
    };
}

/// The outbound HTTP boundary the broker performs token exchanges over. Holds the
/// daemon's blocking `Io`; a fresh `std.http.Client` per call (mints are rare —
/// only a cold-cache miss), mirroring `github_net.download`. No retained socket,
/// so no lifecycle to leak.
pub const HttpClientExchange = struct {
    io: std.Io,
    /// Borrowed process scheduler — the exchange owns no deadline thread.
    sched: *Scheduler,
    /// Outbound deadline (ms). Defaults to the production bound; a test injects a
    /// short value to prove the deadline fires without a 10 s wait.
    deadline_ms: u31 = MINT_DEADLINE_MS,

    pub fn exchange(self: *HttpClientExchange) integration.HttpExchange {
        return .{ .ptr = self, .postFn = postImpl };
    }

    fn postImpl(ptr: *anyopaque, alloc: std.mem.Allocator, req: integration.HttpRequest) anyerror!integration.HttpResponse {
        const self: *HttpClientExchange = @ptrCast(@alignCast(ptr));
        var client: std.http.Client = .{ .allocator = alloc, .io = self.io };
        defer client.deinit();

        // Token-endpoint exchanges (oauth2 refresh) post an unauthenticated
        // form body — the bearer is null there; the App-JWT mints set it.
        const auth: ?[]u8 = if (req.bearer) |b| try std.fmt.allocPrint(alloc, "Bearer {s}", .{b}) else null;
        defer if (auth) |a| alloc.free(a);

        // BUFFER GATE: ArrayList(u8) for the response body — fetch streams into it.
        var body: std.ArrayList(u8) = .empty;
        var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

        var headers: [4]std.http.Header = undefined;
        var n: usize = 0;
        if (auth) |a| {
            headers[n] = .{ .name = "authorization", .value = a };
            n += 1;
        }
        headers[n] = .{ .name = "accept", .value = req.accept };
        n += 1;
        headers[n] = .{ .name = "user-agent", .value = req.user_agent };
        n += 1;
        if (req.body.len > 0) {
            headers[n] = .{ .name = "content-type", .value = req.content_type };
            n += 1;
        }

        // Deadline-arm the exchange, fail CLOSED. postImpl runs concurrently
        // across workspaces; each call owns its own generation, so one call's
        // deadline can never reach another's socket. A pin/arm failure refuses
        // the call rather than running unbounded — same discipline as
        // `bounded_fetch`.
        var owner: call_deadline.SocketOwner = .{};
        const generation = owner.beginAttempt();
        const handle = http_pin.pinPooledHandle(&client, req.url) orelse return error.HttpExchangeFailed;
        _ = owner.attachSocket(generation, handle);
        var guard = self.sched.arm(owner.target(generation), self.deadline_ms) catch
            return error.HttpExchangeFailed;
        defer {
            owner.endAttempt();
            _ = guard.finish();
        }

        const result = client.fetch(.{
            .location = .{ .url = req.url },
            .method = .POST,
            .payload = if (req.body.len > 0) req.body else null,
            .extra_headers = headers[0..n],
            // A redirect's new leg dials a fresh socket outside the armed handle
            // (re-opening the unbounded window); token endpoints answer directly.
            .redirect_behavior = .unhandled,
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
        log.info(S_GITHUB_UNCONFIGURED, .{ .reason = R_VAULT_MISS });
        return null;
    };
    defer parsed.deinit();

    const obj = parsed.value.object;
    const app_id_v = obj.get(FIELD_APP_ID) orelse return logMissing("app_id");
    const pem_v = obj.get(FIELD_PRIVATE_KEY_PEM) orelse return logMissing("private_key_pem");
    if (app_id_v != .string or pem_v != .string) return logMissing(F_NON_STRING);

    const app_id = alloc.dupe(u8, app_id_v.string) catch return null;
    const pem = alloc.dupe(u8, pem_v.string) catch {
        alloc.free(app_id);
        return null;
    };
    const slug: ?[]const u8 = if (obj.get(FIELD_APP_SLUG)) |v|
        (if (v == .string) (alloc.dupe(u8, v.string) catch null) else null)
    else
        null;
    log.info("credential_broker_github_configured", .{ .app_id = app_id });
    return .{ .app_id = app_id, .private_key_pem = pem, .app_slug = slug };
}

fn logMissing(field: []const u8) ?integration.GithubApp {
    log.info(S_GITHUB_UNCONFIGURED, .{ .reason = R_MISSING_FIELD, .field = field });
    return null;
}

/// Load an OAuth platform app `{client_id, client_secret}` from `(admin_ws_id,
/// <provider>-app)` — the same admin-workspace bag the connect flow reads
/// (`connectors/oauth2.zig`). Both strings duped into `alloc` (process-lifetime;
/// the broker signs no JWT here, it posts them to the token endpoint on each cold
/// refresh mint). Degrades to `null` on ANY miss so an unconfigured provider still
/// boots (its mints return `reconnect_required`). Secrets are never logged (VLT).
fn loadOauthApp(alloc: std.mem.Allocator, conn: *pg.Conn, admin_ws_id: []const u8, key: []const u8) ?integration.OauthApp {
    var parsed = vault.loadJson(alloc, conn, admin_ws_id, key) catch {
        log.info(S_OAUTH_UNCONFIGURED, .{ .reason = R_VAULT_MISS, .key = key });
        return null;
    };
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return logOauthMissing(key, "non_object"),
    };
    const cid_v = obj.get(FIELD_CLIENT_ID) orelse return logOauthMissing(key, "client_id");
    const csec_v = obj.get(FIELD_CLIENT_SECRET) orelse return logOauthMissing(key, "client_secret");
    if (cid_v != .string or csec_v != .string) return logOauthMissing(key, F_NON_STRING);

    const cid = alloc.dupe(u8, cid_v.string) catch return null;
    const csec = alloc.dupe(u8, csec_v.string) catch {
        alloc.free(cid);
        return null;
    };
    log.info("credential_broker_oauth_configured", .{ .key = key });
    return .{ .client_id = cid, .client_secret = csec };
}

fn logOauthMissing(key: []const u8, field: []const u8) ?integration.OauthApp {
    log.info(S_OAUTH_UNCONFIGURED, .{ .reason = R_MISSING_FIELD, .key = key, .field = field });
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

// Tests live in `serve_broker_test.zig` (FLL-exempt) — production stays ≤350.
