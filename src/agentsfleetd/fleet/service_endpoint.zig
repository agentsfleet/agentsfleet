//! Custom-endpoint resolution for the lease wire — extracted from
//! `service.zig` (RULE FLL split): the provider-name/egress-host/URL
//! triple `resolveExecutionPolicy` folds into the `ExecutionPolicy`.

const std = @import("std");
const logging = @import("log");
const wire = @import("contract");
const execution_policy = wire.execution_policy;
const ec = @import("../errors/error_registry.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const log = logging.scoped(.runner_lease);

/// The lease's provider name + egress host + dialed URL, branching on whether the
/// resolved credential is a custom OpenAI-compatible endpoint:
///   - custom (base_url set): hand nullclaw the `custom:<url>` provider name (so
///     it classifies as `.compatible_provider` and honours the URL override —
///     NEVER "openai"), carry the URL as `base_url`, and derive the egress
///     `inference_host` from the SAME URL so the allowlist permits exactly it.
///   - named provider (base_url null): pass the provider through unchanged with
///     no base_url; `inference_host` stays "" exactly as before — named-provider
///     leases are byte-for-byte unchanged (Invariant 7).
/// Arena-scoped (`alloc` is `hx.alloc`); the `custom:<url>` name + host live until
/// `hx.ok` serializes. An OOM building the custom name degrades to the SAME shape
/// the named-provider branch returns — the raw provider with NO base_url and an
/// empty inference_host — so nullclaw never receives the bare `openai-compatible`
/// id paired with a URL (an undefined route: `classifyProvider` maps it to no
/// documented provider). With no base_url it classifies as a plain unknown named
/// provider and the engine fails authentication predictably, matching the clean
/// failure of the `resolved == null` / no-custom-endpoint branches.
pub fn customEndpoint(
    alloc: std.mem.Allocator,
    resolved: ?tenant_provider.ResolvedProvider,
) struct { provider: []const u8, base_url: ?[]const u8, inference_host: []const u8 } {
    const r = resolved orelse return .{ .provider = "", .base_url = null, .inference_host = "" };
    const base_url = r.base_url orelse return .{ .provider = r.provider, .base_url = null, .inference_host = "" };

    const custom_name = std.fmt.allocPrint(alloc, "{s}{s}", .{ execution_policy.CUSTOM_PROVIDER_PREFIX, base_url }) catch {
        log.warn("lease_custom_provider_name_alloc_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .inference_host = execution_policy.hostFromUrl(base_url) });
        return .{ .provider = r.provider, .base_url = null, .inference_host = "" };
    };
    return .{ .provider = custom_name, .base_url = base_url, .inference_host = execution_policy.hostFromUrl(base_url) };
}

// ── Tests ────────────────────────────────────────────────────────────────────

// `customEndpoint` only reads `provider` / `base_url`, so the test builds a
// ResolvedProvider from borrowed literals (api_key/model are unused here) and
// never deinits it — no allocation owns these bytes.
fn fixedProvider(provider: []const u8, base_url: ?[]const u8) tenant_provider.ResolvedProvider {
    return .{
        .mode = .self_managed,
        .provider = @constCast(provider),
        .api_key = @constCast(""),
        .model = @constCast(""),
        .context_cap_tokens = 0,
        .base_url = if (base_url) |u| @constCast(u) else null,
    };
}

test "customEndpoint: no resolved provider yields an empty, no-endpoint result" {
    const out = customEndpoint(std.testing.allocator, null);
    try std.testing.expectEqualStrings("", out.provider);
    try std.testing.expect(out.base_url == null);
    try std.testing.expectEqualStrings("", out.inference_host);
}

test "customEndpoint: a named provider passes through with no base_url" {
    const out = customEndpoint(std.testing.allocator, fixedProvider("anthropic", null));
    try std.testing.expectEqualStrings("anthropic", out.provider);
    try std.testing.expect(out.base_url == null);
    try std.testing.expectEqualStrings("", out.inference_host);
}

test "customEndpoint: a custom endpoint becomes the custom: provider name + egress host" {
    const out = customEndpoint(std.testing.allocator, fixedProvider(
        tenant_provider.OPENAI_COMPATIBLE_PROVIDER,
        "https://vllm.corp/v1",
    ));
    defer std.testing.allocator.free(out.provider); // the only allocated field
    try std.testing.expectEqualStrings("custom:https://vllm.corp/v1", out.provider);
    try std.testing.expectEqualStrings("https://vllm.corp/v1", out.base_url.?);
    try std.testing.expectEqualStrings("vllm.corp", out.inference_host);
}

test "customEndpoint: an OOM building the custom name fails predictably (no base_url smuggled)" {
    // failing_allocator OOMs the allocPrint; the branch must degrade to the
    // named-provider shape — the bare provider with NO base_url and an empty
    // host — so nullclaw never receives `openai-compatible` paired with a URL
    // (an undefined route). This is the clean failure the doc comment promises.
    const out = customEndpoint(std.testing.failing_allocator, fixedProvider(
        tenant_provider.OPENAI_COMPATIBLE_PROVIDER,
        "https://vllm.corp/v1",
    ));
    try std.testing.expectEqualStrings(tenant_provider.OPENAI_COMPATIBLE_PROVIDER, out.provider);
    try std.testing.expect(out.base_url == null);
    try std.testing.expectEqualStrings("", out.inference_host);
}
