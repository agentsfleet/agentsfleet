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
const credentials_integration = @import("../../../credentials/integration.zig");
const oauth2 = @import("oauth2.zig");
const oauth_status = @import("oauth_status.zig");
const connector_state = @import("state.zig");
const slack_spec = @import("slack/spec.zig");
const slack_callback = @import("slack/callback.zig");
const slack_status = @import("slack/status.zig");
const github_spec = @import("github/spec.zig");
const github_connect = @import("github/connect.zig");
const github_callback = @import("github/callback.zig");
const github_status = @import("github/status.zig");
const zoho_spec = @import("zoho/spec.zig");
const zoho_callback = @import("zoho/callback.zig");
const zoho_multi_dc = @import("zoho/multi_dc.zig");
const jira_spec = @import("jira/spec.zig");
const jira_callback = @import("jira/callback.zig");
const linear_spec = @import("linear/spec.zig");
const linear_callback = @import("linear/callback.zig");

const Hx = hx_mod.Hx;

/// oauth2 post-auth hook: parse the exchange body + persist the install rows.
/// `location` is the callback's `location` query param, when the provider's
/// redirect carries one (Zoho multi-DC: "us"/"eu"/"in"/"au"/"cn"/"jp"/"ca");
/// null for every other provider. Never writes the response — the generic
/// callback owns response mapping.
pub const PostAuthFn = *const fn (hx: Hx, workspace_id: []const u8, exchange_body: []const u8, location: ?[]const u8) anyerror!void;
/// app_install completion hook: validates + persists AND owns its failure
/// responses (an installation callback's inputs are provider-bespoke). The
/// raw state is passed through so the provider can keep freshness checks
/// adjacent to its final persistence.
/// Returns true on success; false after having responded.
pub const CompleteFn = *const fn (hx: Hx, workspace_id: []const u8, raw_state: []const u8, req: *httpz.Request) bool;
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
    /// Multi-DC providers (Zoho) override the exchange's effective token
    /// endpoint from the callback's `location` query param — the code is
    /// only redeemable at the data-center-specific accounts server that
    /// issued it, not the single-region `flow.token_endpoint`. Single-region
    /// providers (Slack/Jira/Linear) leave this null and always use
    /// `flow.token_endpoint`.
    resolve_token_endpoint: ?*const fn (location: ?[]const u8) []const u8 = null,
};

/// App-installation archetype (GitHub App shape): no code exchange — the
/// callback carries an installation id and writes only the vault handle.
pub const AppInstallData = struct {
    state: connector_state.Config,
    build_install_url: BuildInstallUrlFn,
    complete: CompleteFn,
};

/// The strategy tagged-union — owns which flow runs; exhaustive switches in
/// the generic handlers mean a new archetype cannot land half-wired. (An
/// api_key archetype was considered and dropped: a static vendor key is just a
/// workspace secret referenced as `${secrets.<name>.<field>}`, not a connector.)
pub const Archetype = union(enum) {
    oauth2: Oauth2Data,
    app_install: AppInstallData,
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
    .{
        .provider = common.PROVIDER_ZOHO,
        .display_name = "Zoho Desk",
        .archetype = .{ .oauth2 = .{
            .flow = zoho_spec.SPEC,
            .refresh = true,
            .exchange_failed_code = ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED,
            .post_auth = zoho_callback.postAuth,
            .resolve_token_endpoint = zoho_multi_dc.tokenEndpoint,
        } },
        .respond_status = oauth_status.respondStatus,
    },
    .{
        .provider = common.PROVIDER_JIRA,
        .display_name = "Jira",
        .archetype = .{ .oauth2 = .{
            .flow = jira_spec.SPEC,
            .refresh = true,
            .exchange_failed_code = ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED,
            .post_auth = jira_callback.postAuth,
        } },
        .respond_status = oauth_status.respondStatus,
    },
    .{
        .provider = common.PROVIDER_LINEAR,
        .display_name = "Linear",
        .archetype = .{ .oauth2 = .{
            .flow = linear_spec.SPEC,
            .refresh = true,
            .exchange_failed_code = ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED,
            .post_auth = linear_callback.postAuth,
        } },
        .respond_status = oauth_status.respondStatus,
    },
};

/// The signed single-use state binding an archetype carries (oauth2 via its
/// flow, app_install directly). Both remaining archetypes carry a
/// `connector_state.Config` — the SOLE trust anchor of the Bearer-less callback.
fn stateBinding(spec: ConnectorSpec) ?connector_state.Config {
    return switch (spec.archetype) {
        .oauth2 => |o| o.flow.state,
        .app_install => |a| a.state,
    };
}

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
        // Every callback-bearing archetype needs a non-empty state binding —
        // it is the SOLE trust anchor of the Bearer-less callback (state.zig),
        // so an empty domain/nonce is a degenerate HMAC domain, not a typo.
        // (oauth2 was previously unchecked; only app_install was.)
        if (stateBinding(spec)) |sb| {
            if (sb.domain_prefix.len == 0 or sb.nonce_prefix.len == 0)
                @compileError("registry: entry without a state binding (domain/nonce): " ++ spec.provider);
        }
        switch (spec.archetype) {
            .oauth2 => |o| {
                if (o.flow.scopes.len == 0) @compileError("registry: oauth2 entry without scopes: " ++ spec.provider);
                if (!std.mem.eql(u8, o.flow.provider, spec.provider))
                    @compileError("registry: oauth2 flow provider id disagrees with entry: " ++ spec.provider);
                if (o.exchange_failed_code.len == 0) @compileError("registry: oauth2 entry without exchange_failed_code: " ++ spec.provider);
                // Finding ① drift guard: a refresh-token connector is useless
                // without a broker refresh-mint entry to turn its vaulted refresh
                // token into access tokens. Prove the two registries agree at
                // compile time — the connector registry (this file) is the higher
                // layer, so it checks the lower `credentials/integration.zig`.
                if (o.refresh and !credentials_integration.hasRefreshMint(spec.provider))
                    @compileError("registry: refresh connector '" ++ spec.provider ++ "' has no oauth2_refresh entry in credentials/integration.zig — the broker cannot mint from it");
            },
            .app_install => {},
        }
    }
    // State bindings must be UNIQUE across entries. domain_prefix is
    // independent of the provider id, so the duplicate-id scan above does not
    // catch two connectors sharing a state domain — and a shared (domain,
    // nonce) pair lets one connector's signed state verify + consume under
    // another's callback (state.zig's cross-verify invariant). Enforce it here.
    for (REGISTRY, 0..) |spec, i| {
        const sb = stateBinding(spec) orelse continue;
        for (REGISTRY[i + 1 ..]) |other| {
            const ob = stateBinding(other) orelse continue;
            const pair = spec.provider ++ PROVIDER_PAIR_SEP ++ other.provider;
            if (std.mem.eql(u8, sb.domain_prefix, ob.domain_prefix))
                @compileError("registry: duplicate state domain_prefix across providers: " ++ pair);
            if (std.mem.eql(u8, sb.nonce_prefix, ob.nonce_prefix))
                @compileError("registry: duplicate state nonce_prefix across providers: " ++ pair);
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

/// Separator for the "<a> / <b>" provider pair in the duplicate-binding
/// comptime error messages.
const PROVIDER_PAIR_SEP = " / ";

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

    const zoho = lookup(common.PROVIDER_ZOHO) orelse return error.TestUnexpectedResult;
    try testing.expect(zoho.archetype == .oauth2);
    try testing.expect(zoho.archetype.oauth2.refresh);

    const jira = lookup(common.PROVIDER_JIRA) orelse return error.TestUnexpectedResult;
    try testing.expect(jira.archetype == .oauth2);
    try testing.expect(jira.archetype.oauth2.refresh);

    const linear = lookup(common.PROVIDER_LINEAR) orelse return error.TestUnexpectedResult;
    try testing.expect(linear.archetype == .oauth2);
    try testing.expect(linear.archetype.oauth2.refresh);
}

test "registry: unknown or empty provider resolves to null (the 404 path)" {
    try testing.expect(lookup("nope") == null);
    try testing.expect(lookup("") == null);
    try testing.expect(lookup("SLACK") == null); // ids are exact, lowercase
}

test "registry: exactly the shipped entries (a new provider updates this pin)" {
    // Pin test: the registry is the provider catalog source of truth. Five OAuth
    // connectors — api-key providers (Datadog/Grafana/Fly) are custom secrets now.
    try testing.expectEqual(@as(usize, 5), REGISTRY.len);
}
