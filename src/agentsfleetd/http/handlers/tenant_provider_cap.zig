//! Context-cap resolution for a self-managed provider activation.
//!
//! Split out of `tenant_provider.zig` at the 350-line cap (RULE FLL). The seam is
//! by question: that file owns the GET/PUT/DELETE request shape, this one owns
//! the single question "what context window does this (provider, model) get, and
//! is the model catalogued at all" — which is the gate that decides whether an
//! activation is accepted (UZ-PROVIDER-004).

const std = @import("std");
const pg = @import("pg");

const tenant_provider = @import("../../state/tenant_provider.zig");
const model_rate_cache = @import("../../state/model_rate_cache.zig");
const revision_state = @import("../../state/model_catalogue_revision.zig");

/// Context-cap persisted for a custom (openai-compatible) self-managed endpoint.
/// A custom endpoint bills provider-direct — self_managed posture charges a
/// run-fee only and never reads the per-token rate cache — so its user-hosted
/// model is absent from core.model_library by design and there is no platform rate
/// to catalogue. The activation gate stores this "unknown/auto" sentinel instead
/// of a catalogue lookup; execution_policy.autoToolWindow + the per-fleet
/// frontmatter overlay resolve the effective context window at run time.
const CUSTOM_ENDPOINT_CAP_UNKNOWN: u32 = 0;

/// Providers that serve models from hardware the operator owns. NullClaw dials
/// each by name at a localhost endpoint and carries NO model list for any of
/// them — `providers/factory.zig` gives them a name, a URL and a display label,
/// nothing more — because there cannot be one: the served model is whatever the
/// operator loaded (`--served-model-name`, an `ollama pull`). The set is
/// unbounded and per-install.
///
/// So `core.model_library` can never hold the (provider, model) pair a real
/// request names, and enforcing membership refuses EVERY local activation —
/// which it did. Seeding a placeholder model id does not fix it either: any
/// fixed string is a guess about what the operator called their model.
///
/// They take the custom-endpoint path instead, for the same reason it exists: a
/// user-hosted model is absent from the platform catalogue by design. Billing is
/// unaffected — a local runtime is self-managed by construction, and
/// self-managed charges a run fee only, never a token rate.
///
/// Kept in lockstep with the allowlist's `rate_basis: "activation_floor"` set by
/// scripts/check_model_allowlist.py, so the two cannot drift apart silently.
const LOCAL_RUNTIME_PROVIDERS = [_][]const u8{
    "litellm",
    "llama.cpp",
    "llamacpp",
    "lm-studio",
    "lmstudio",
    "ollama",
    "osaurus",
    "sglang",
    "vllm",
};

/// Whether this provider serves models from the operator's own hardware.
fn isLocalRuntime(provider: []const u8) bool {
    for (LOCAL_RUNTIME_PROVIDERS) |name| {
        if (std.mem.eql(u8, provider, name)) return true;
    }
    return false;
}

/// Resolve the context-window cap to persist for a self-managed activation.
/// A custom (openai-compatible) endpoint is provider-direct billing: its
/// user-hosted model is absent from the platform rate catalogue by design, so it
/// bypasses the gate and takes the unknown/auto sentinel. A local runtime
/// (`LOCAL_RUNTIME_PROVIDERS`) takes the same path for the same reason — the
/// model lives on the operator's hardware and cannot be enumerated. A named provider must
/// resolve a catalogued rate row (whose cap we store) — `null` means the model is
/// not in the catalogue, and the caller fails it (UZ-PROVIDER-004). The rate row
/// is keyed by (provider, model): the credential's provider is the authority for
/// which provider hosts the model.
///
/// A blank, whitespace-only, OR whitespace-padded effective model returns `null`
/// for EVERY provider — the credential no longer guarantees a model (M121: it
/// lives on the registry entry / PUT body), so this is the boundary that
/// re-establishes "an activation must name a usable model." Without it a bare PUT
/// for an openai-compatible secret takes the sentinel path and persists a blank or
/// whitespace-padded model the endpoint can't dial (named providers already miss
/// the catalogue lookup). Rejecting padded input — rather than silently trimming
/// it — keeps both provider kinds consistent and surfaces the typo to the caller.
pub fn resolveSelfManagedCap(conn: *pg.Conn, provider: []const u8, model: []const u8) ?u32 {
    const trimmed = std.mem.trim(u8, model, &std.ascii.whitespace);
    if (trimmed.len == 0 or trimmed.len != model.len) return null;
    if (std.mem.eql(u8, provider, tenant_provider.OPENAI_COMPATIBLE_PROVIDER) or isLocalRuntime(provider)) {
        // A custom endpoint still names a real model, and a context window is a
        // property of the MODEL, not the host serving it — so borrow the
        // catalogue's cap when it knows one (never the rate: self-managed is
        // billed by the tenant's own provider). The sentinel fallback keeps a
        // genuinely unknown model activating exactly as before, and now also
        // absorbs a failed read: an activation must not be refused because the
        // catalogue was briefly unreachable.
        const cap = model_rate_cache.contextCapForModel(conn, trimmed) catch null;
        return cap orelse CUSTOM_ENDPOINT_CAP_UNKNOWN;
    }
    // A named provider's model MUST be catalogued — this is the validator that
    // rejects an activation naming one that is not (UZ-PROVIDER-004). It asks
    // the database rather than a cache: under the old snapshot cache an evicted
    // or not-yet-loaded row read as "not in the catalogue" and rejected a
    // perfectly valid activation.
    const revision = revision_state.read(conn) catch return null;
    const entry = (model_rate_cache.rateAtRevision(conn, revision, provider, model) catch return null) orelse return null;
    return entry.context_cap_tokens;
}

test "every local runtime is recognised" {
    for (LOCAL_RUNTIME_PROVIDERS) |name| {
        try std.testing.expect(isLocalRuntime(name));
    }
}

test "a hosted provider is not a local runtime" {
    // These MUST keep enforcing catalogue membership: they are billable under
    // platform posture, so an uncatalogued model has to fail closed.
    for ([_][]const u8{ "fireworks", "anthropic", "openai", "kimi", "bedrock", "pioneer" }) |name| {
        try std.testing.expect(!isLocalRuntime(name));
    }
}

test "local-runtime matching is exact, never a prefix or case fold" {
    // A near-miss must not silently buy a catalogue bypass.
    for ([_][]const u8{ "", "vllm2", "xvllm", "VLLM", "Ollama", "llama.cp", "llama.cppx", "lm-studio " }) |name| {
        try std.testing.expect(!isLocalRuntime(name));
    }
}
