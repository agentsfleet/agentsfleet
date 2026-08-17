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

/// Resolve the context-window cap to persist for a self-managed activation.
/// A custom (openai-compatible) endpoint is provider-direct billing: its
/// user-hosted model is absent from the platform rate catalogue by design, so it
/// bypasses the gate and takes the unknown/auto sentinel. A named provider must
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
    if (std.mem.eql(u8, provider, tenant_provider.OPENAI_COMPATIBLE_PROVIDER)) {
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
