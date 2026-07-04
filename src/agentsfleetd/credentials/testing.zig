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
    resp_body: []const u8 = "{\"token\":\"ghs_minted\"}",
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
pub fn githubCtx(alloc: std.mem.Allocator, handle: std.json.Value, gh: *FakeGitHub, now_ms: i64) MintCtx {
    return .{
        .alloc = alloc,
        .handle = handle,
        .now_ms = now_ms,
        .platform = .{ .github = fake_app },
        .http = gh.exchange(),
        .sign = fakeSign,
    };
}

/// Recording metrics sink (#11 tests): captures every emitted `MintEvent`.
pub const RecordingMetrics = struct {
    count: usize = 0,
    last_outcome: []const u8 = "",
    last_hit: bool = false,

    fn onMint(ptr: *anyopaque, ev: integration.MintEvent) void {
        const self: *RecordingMetrics = @ptrCast(@alignCast(ptr));
        self.count += 1;
        self.last_outcome = ev.outcome;
        self.last_hit = ev.cache_hit;
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
