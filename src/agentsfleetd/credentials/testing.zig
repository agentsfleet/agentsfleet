//! Shared test doubles for credential-integration tests. With these, a new
//! integration's unit tests are a few lines (#6) and a failure case is one field
//! (#8): a fake GitHub HTTP boundary (request capture + injectable error/latency),
//! a fake RS256 signer, a recording metrics sink, and MintCtx/Deps builders.
//!
//! Test-only module — referenced from other files' `test` blocks, never the
//! production graph.

const std = @import("std");
const integration = @import("integration.zig");

pub const MintCtx = integration.MintCtx;
pub const Deps = integration.Deps;

/// A default fake GitHub App key — a distinctive non-secret marker so the
/// key-never-leaks tests can assert it is absent from every outbound surface.
pub const fake_app = integration.GithubApp{
    .app_id = "123456",
    .private_key_pem = "FAKE_PRIVATE_KEY_MATERIAL_zzz",
};

pub const fake_oauth_app = integration.OauthApp{
    .client_id = "oauth-client-id",
    .client_secret = "oauth-client-secret",
};

/// Fake RS256 signer — returns a fixed marker (real signing is proven in
/// `rs256_sign.zig`); integration tests exercise assembly + exchange, not crypto.
pub fn fakeSign(out: []u8, private_key_pem: []const u8, signing_input: []const u8) anyerror![]const u8 {
    _ = private_key_pem;
    _ = signing_input;
    const marker = "FAKESIG";
    @memcpy(out[0..marker.len], marker);
    return out[0..marker.len];
}

/// Parse a JSON handle for a test; caller `defer`s `.deinit()`.
pub fn parse(alloc: std.mem.Allocator, comptime json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

/// A `MintCtx` over `handle` whose effects all refuse — for integrations that
/// touch neither http nor sign (the `static` path).
pub fn ctxOver(alloc: std.mem.Allocator, handle: std.json.Value) MintCtx {
    const d = integration.nullDeps();
    return .{ .alloc = alloc, .handle = handle, .now_ms = 0, .platform = d.platform, .http = d.http, .sign = d.sign };
}

/// Fake GitHub: replies with a canned status + body and captures the outbound url
/// + bearer. Set `fail_with` to inject a transport error (#8 failure injection).
pub const FakeGitHub = struct {
    alloc: std.mem.Allocator,
    status: u16 = 201,
    /// A real create-installation-access-token response echoes the repositories
    /// the token was granted, and the mint refuses a token whose stated reach is
    /// not the declared binding (`integration_github_reach.zig`). So the default
    /// body states the reach `test_binding` declares — a fake that omitted it
    /// would make every mint fail closed for a reason no test was about.
    resp_body: []const u8 = "{\"token\":\"ghs_minted\",\"repositories\":[{\"full_name\":\"acme/widgets\"}],\"permissions\":{\"contents\":\"write\",\"pull_requests\":\"write\"}}",
    fail_with: ?anyerror = null,
    calls: usize = 0,
    url: []u8 = &.{},
    bearer: []u8 = &.{},
    body: []u8 = &.{},

    fn post(ptr: *anyopaque, alloc: std.mem.Allocator, req: integration.HttpRequest) anyerror!integration.HttpResponse {
        const self: *FakeGitHub = @ptrCast(@alignCast(ptr));
        self.calls += 1;
        if (self.fail_with) |e| return e;
        if (self.url.len != 0) self.alloc.free(self.url);
        if (self.bearer.len != 0) self.alloc.free(self.bearer);
        if (self.body.len != 0) self.alloc.free(self.body);
        self.url = try self.alloc.dupe(u8, req.url);
        self.bearer = try self.alloc.dupe(u8, req.bearer orelse "");
        self.body = try self.alloc.dupe(u8, req.body);
        return .{ .status = self.status, .body = try alloc.dupe(u8, self.resp_body) };
    }

    pub fn exchange(self: *FakeGitHub) integration.HttpExchange {
        return .{ .ptr = self, .postFn = post };
    }

    pub fn deinit(self: *FakeGitHub) void {
        if (self.url.len != 0) self.alloc.free(self.url);
        if (self.bearer.len != 0) self.alloc.free(self.bearer);
        if (self.body.len != 0) self.alloc.free(self.body);
    }
};

/// A `MintCtx` wired with a fake GitHub + fake signer + the fake App key.
/// A bound repository set for tests whose subject is something other than the
/// binding (status mapping, JWT shape, key hygiene). A GitHub mint fails closed
/// without one, so those tests would otherwise all assert the refusal instead of
/// what they are about. Tests OF the binding pass their own via `githubCtxBound`.
pub const TEST_REPOSITORIES = [_][]const u8{"acme/widgets"};
pub const test_binding: integration.RepositoryBinding = .{
    .repositories = &TEST_REPOSITORIES,
    .access = .write,
};

/// A create-installation-access-token response stating that the minted token
/// reaches exactly `declared` at `access` level. The mint verifies both the
/// stated reach AND the stated permissions against the fleet's binding
/// (`integration_github_reach.zig`), so a test whose fleet declares something
/// other than `test_binding` must answer with its own reach or the mint
/// refuses for a reason that test is not about. Caller owns.
pub fn reachResponse(alloc: std.mem.Allocator, declared: []const []const u8, access: integration.RepositoryAccess) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    try out.appendSlice(alloc, "{\"token\":\"ghs_minted\",\"repositories\":[");
    for (declared, 0..) |full_name, i| {
        if (i > 0) try out.append(alloc, ',');
        const entry = try std.fmt.allocPrint(alloc, "{{\"full_name\":\"{s}\"}}", .{full_name});
        defer alloc.free(entry);
        try out.appendSlice(alloc, entry);
    }
    try out.appendSlice(alloc, switch (access) {
        .read => "],\"permissions\":{\"contents\":\"read\"}}",
        .write => "],\"permissions\":{\"contents\":\"write\",\"pull_requests\":\"write\"}}",
    });
    return out.toOwnedSlice(alloc);
}

pub fn githubCtx(alloc: std.mem.Allocator, handle: std.json.Value, gh: *FakeGitHub, now_ms: i64) MintCtx {
    return githubCtxBound(alloc, handle, gh, now_ms, test_binding);
}

/// `githubCtx` with an explicit repository binding — pass null to exercise the
/// unbound refusal.
pub fn githubCtxBound(
    alloc: std.mem.Allocator,
    handle: std.json.Value,
    gh: *FakeGitHub,
    now_ms: i64,
    binding: ?integration.RepositoryBinding,
) MintCtx {
    return .{
        .alloc = alloc,
        .handle = handle,
        .now_ms = now_ms,
        .platform = .{ .github = fake_app },
        .http = gh.exchange(),
        .sign = fakeSign,
        .repository_binding = binding,
    };
}

/// Recording metrics sink (#11 tests): captures every emitted `MintEvent`.
pub const RecordingMetrics = struct {
    count: usize = 0,
    last_outcome: []const u8 = "",
    last_hit: bool = false,
    last_latency_ms: i64 = 0,

    fn onMint(ptr: *anyopaque, ev: integration.MintEvent) void {
        const self: *RecordingMetrics = @ptrCast(@alignCast(ptr));
        self.count += 1;
        self.last_outcome = ev.outcome;
        self.last_hit = ev.cache_hit;
        self.last_latency_ms = ev.latency_ms;
    }

    pub fn sink(self: *RecordingMetrics) integration.Metrics {
        return .{ .ptr = self, .onMintFn = onMint };
    }
};

/// Broker `Deps` wired with a fake GitHub + fake signer + fake key + a metrics
/// sink — for broker wiring / integration-tier tests (#8).
pub fn brokerDeps(gh: *FakeGitHub, metrics: *RecordingMetrics) Deps {
    return .{
        .platform = .{ .github = fake_app, .zoho = fake_oauth_app, .jira = fake_oauth_app, .linear = fake_oauth_app },
        .http = gh.exchange(),
        .sign = fakeSign,
        .metrics = metrics.sink(),
    };
}
