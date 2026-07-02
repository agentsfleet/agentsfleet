//! The comptime connector registry — adding a provider to agentsfleet is ONE
//! `ConnectorSpec` entry here (plus its per-provider hook file), never new
//! route or flow code. The generic `{provider}` handlers
//! (`connectors/{connect,callback,status}.zig`) resolve the captured route
//! segment against this table and dispatch on the archetype; callers switch
//! on SHAPE, never on provider id (the strategy tagged-union owns its
//! dispatch — RULE TGU).
//!
//! Comptime validation below makes the table's invariants compile-time facts:
//! a duplicate provider id, an empty id, an oauth2 entry without scopes, or a
//! flow whose provider id disagrees with its entry is a COMPILE ERROR, not a
//! runtime surprise.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const oauth2 = @import("oauth2.zig");
const connector_state = @import("state.zig");
const slack_spec = @import("slack/spec.zig");
const slack_callback = @import("slack/callback.zig");
const slack_status = @import("slack/status.zig");
const github_spec = @import("github/spec.zig");
const github_connect = @import("github/connect.zig");
const github_callback = @import("github/callback.zig");
const github_status = @import("github/status.zig");

const Hx = hx_mod.Hx;

/// oauth2 post-auth hook: parse the exchange body + persist the install rows.
/// Never writes the response — the generic callback owns response mapping.
pub const PostAuthFn = *const fn (hx: Hx, workspace_id: []const u8, exchange_body: []const u8) anyerror!void;
/// app_install completion hook: validates + persists AND owns its failure
/// responses (an installation callback's inputs are provider-bespoke).
/// Returns true on success; false after having responded.
pub const CompleteFn = *const fn (hx: Hx, workspace_id: []const u8, req: *httpz.Request) bool;
/// Status renderer: `handle` is the parsed `fleet:<provider>` vault object
/// (null = missing/unreadable). Owns the full response body.
pub const RespondStatusFn = *const fn (hx: Hx, handle: ?std.json.ObjectMap) void;
/// app_install install-URL builder (platform config → browser redirect URL).
pub const BuildInstallUrlFn = *const fn (hx: Hx, state: []const u8) error{ NotConfigured, OutOfMemory }![]const u8;

/// OAuth-2.0 authorization-code archetype: connect mints a signed state and
/// redirects to the provider's authorize endpoint; the callback exchanges the
/// `code` (deadline-armed) and the hook persists the vaulted token.
pub const Oauth2Data = struct {
    /// Endpoints + scopes + state binding — the shared flow's data.
    flow: oauth2.Spec,
    /// Whether the vendor issues refresh tokens the broker re-mints from
    /// (consumed by the upcoming provider entries; slack's bot token is
    /// long-lived, so false).
    refresh: bool,
    /// Provider-specific error code for a rejected/malformed exchange.
    exchange_failed_code: []const u8,
    post_auth: PostAuthFn,
};

/// App-installation archetype (GitHub App shape): no code exchange — the
/// callback carries an installation id and writes only the vault handle.
pub const AppInstallData = struct {
    state: connector_state.Config,
    build_install_url: BuildInstallUrlFn,
    complete: CompleteFn,
};

/// User-supplied-key archetype: the operator pastes their own vendor key at
/// connect; no platform app, no browser round-trip, no callback. The first
/// entries land with the provider batch — until then the comptime check
/// below asserts the registry carries none, which is what makes the generic
/// handlers' `.api_key => unreachable` arms provably safe.
pub const ApiKeyData = struct {
    /// Vault-handle field the user's key is stored under.
    key_field: []const u8,
};

/// The strategy tagged-union — owns which flow runs; exhaustive switches in
/// the generic handlers mean a new archetype cannot land half-wired.
pub const Archetype = union(enum) {
    oauth2: Oauth2Data,
    app_install: AppInstallData,
    api_key: ApiKeyData,
};

pub const ConnectorSpec = struct {
    /// Registry id = the `{provider}` route segment = the `provider` column
    /// value = the `<provider>-app` / `fleet:<provider>` vault-key stem.
    /// Always a `common` constant (RULE UFS).
    provider: []const u8,
    /// Human name for operator-facing error details ("Slack connect is not
    /// configured on this deployment").
    display_name: []const u8,
    archetype: Archetype,
    respond_status: RespondStatusFn,
};

pub const REGISTRY = [_]ConnectorSpec{
    .{
        .provider = common.PROVIDER_SLACK,
        .display_name = "Slack",
        .archetype = .{ .oauth2 = .{
            .flow = slack_spec.SPEC,
            .refresh = false,
            .exchange_failed_code = ec.ERR_SLACK_OAUTH_EXCHANGE_FAILED,
            .post_auth = slack_callback.postAuth,
        } },
        .respond_status = slack_status.respondStatus,
    },
    .{
        .provider = common.PROVIDER_GITHUB,
        .display_name = "GitHub",
        .archetype = .{ .app_install = .{
            .state = github_spec.STATE,
            .build_install_url = github_connect.buildInstallUrl,
            .complete = github_callback.complete,
        } },
        .respond_status = github_status.respondStatus,
    },
};

comptime {
    // Registry length × string scans — stay clear of the default 1000 quota.
    @setEvalBranchQuota(REGISTRY.len * REGISTRY.len * 64 + 4096);
    for (REGISTRY, 0..) |spec, i| {
        if (spec.provider.len == 0) @compileError("registry: empty provider id");
        if (spec.display_name.len == 0) @compileError("registry: empty display_name for " ++ spec.provider);
        // Unique ids — a duplicate entry is a compile error, never a
        // first-match-wins runtime surprise.
        for (REGISTRY[i + 1 ..]) |other| {
            if (std.mem.eql(u8, spec.provider, other.provider))
                @compileError("registry: duplicate provider id: " ++ spec.provider);
        }
        switch (spec.archetype) {
            .oauth2 => |o| {
                if (o.flow.scopes.len == 0) @compileError("registry: oauth2 entry without scopes: " ++ spec.provider);
                if (!std.mem.eql(u8, o.flow.provider, spec.provider))
                    @compileError("registry: oauth2 flow provider id disagrees with entry: " ++ spec.provider);
                if (o.exchange_failed_code.len == 0) @compileError("registry: oauth2 entry without exchange_failed_code: " ++ spec.provider);
            },
            .app_install => |a| {
                if (a.state.domain_prefix.len == 0 or a.state.nonce_prefix.len == 0)
                    @compileError("registry: app_install entry without state binding: " ++ spec.provider);
            },
            // No api_key entries yet — the generic handlers' `.api_key =>
            // unreachable` arms rest on this assert; the first api_key
            // provider deletes it and implements the arms in the same diff.
            .api_key => @compileError("registry: api_key entries are not wired yet (implement the generic handlers' api_key arms first): " ++ spec.provider),
        }
    }
}

/// Resolve a captured `{provider}` route segment. Null → the generic
/// handlers 404 with a body naming the unknown provider.
pub fn lookup(provider: []const u8) ?*const ConnectorSpec {
    for (&REGISTRY) |*spec| {
        if (std.mem.eql(u8, spec.provider, provider)) return spec;
    }
    return null;
}

const UNKNOWN_PROVIDER_DETAIL_FMT = "Unknown connector provider: {s}";
const UNKNOWN_PROVIDER_DETAIL_FALLBACK = "Unknown connector provider";

/// The shared 404 for an unresolved `{provider}` segment — names the unknown
/// provider in the body (the segment is caller-supplied; `hx.fail` JSON-escapes
/// the detail). One site so all three generic routes answer identically.
pub fn respondUnknown(hx: Hx, provider: []const u8) void {
    const detail = std.fmt.allocPrint(hx.alloc, UNKNOWN_PROVIDER_DETAIL_FMT, .{provider}) catch
        return hx.fail(ec.ERR_CONNECTOR_UNKNOWN, UNKNOWN_PROVIDER_DETAIL_FALLBACK);
    defer hx.alloc.free(detail);
    hx.fail(ec.ERR_CONNECTOR_UNKNOWN, detail);
}

// ── Tests ────────────────────────────────────────────────────────────────────
// The duplicate-id / missing-scopes cases are compile errors by construction
// (the comptime block above) — they cannot be expressed as runtime test cases;
// this suite pins the runtime lookup surface + the shipped entries' shapes.

const testing = std.testing;

test "registry: lookup resolves the shipped providers to their archetypes" {
    const slack = lookup(common.PROVIDER_SLACK) orelse return error.TestUnexpectedResult;
    try testing.expect(slack.archetype == .oauth2);
    try testing.expect(!slack.archetype.oauth2.refresh);
    try testing.expectEqualStrings("Slack", slack.display_name);

    const github = lookup(common.PROVIDER_GITHUB) orelse return error.TestUnexpectedResult;
    try testing.expect(github.archetype == .app_install);
    try testing.expectEqualStrings("GitHub", github.display_name);
}

test "registry: unknown or empty provider resolves to null (the 404 path)" {
    try testing.expect(lookup("nope") == null);
    try testing.expect(lookup("") == null);
    try testing.expect(lookup("SLACK") == null); // ids are exact, lowercase
}

test "registry: exactly the shipped entries (a new provider updates this pin)" {
    // pin test: literal is the contract
    try testing.expectEqual(@as(usize, 2), REGISTRY.len);
}
