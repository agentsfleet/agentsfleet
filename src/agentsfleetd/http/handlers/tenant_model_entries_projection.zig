//! Row projection for the tenant model registry page — how one
//! `core.tenant_model_entries` row plus its already-batched metadata and rate
//! becomes one wire row.
//!
//! Split from `tenant_model_entries_view.zig` per RULE FLL when that file
//! reached the 350-line cap. The seam is deliberate rather than arithmetic:
//! everything here operates on values already in memory, and nothing here holds
//! a database handle. That is the structural half of the never-decrypt
//! invariant — a module with no connection cannot issue a query, so no future
//! edit can reintroduce a per-row read on this path.

const std = @import("std");
const pg = @import("pg");

const entries_state = @import("../../state/tenant_model_entries.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const vault = @import("../../state/vault.zig");
const metadata = @import("../../secrets/metadata.zig");
const model_rate_batch = @import("../../state/model_rate_batch.zig");

/// One wire row for the `models` array. `kind` is a static `@tagName` slice
/// (never freed); the rest are heap-owned (see `freeView`). No `api_key`
/// field exists — `has_key` is the only signal a caller sees.
pub const EntryView = struct {
    id: []const u8,
    model_id: []const u8,
    secret_ref: []const u8,
    provider: ?[]const u8 = null,
    kind: []const u8,
    base_url: ?[]const u8 = null,
    has_key: bool,
    context_cap_tokens: ?u32 = null,
    input_nanos_per_mtok: ?i64 = null,
    cached_input_nanos_per_mtok: ?i64 = null,
    output_nanos_per_mtok: ?i64 = null,
    active: bool,
    created_at: i64,
};

/// The page's rate lookups, answered in ONE statement whatever the page size.
///
/// Positional, exactly like the `meta` batch above and for the same reason:
/// `slots[i]` belongs to `entries[i]` by construction, and the platform default —
/// when there is one — takes the slot after the last entry. Folding the default
/// in here rather than resolving it on its own is what keeps the whole page at
/// one rate statement instead of two.
///
/// What this replaced: a resident-only cache read. It cost no statement, but it
/// answered null for every row until some unrelated billing charge happened to
/// load that exact `(provider, model)` — so after a restart the Models page
/// showed blank rates indefinitely. Nothing filled the cache for display any
/// more once the boot warm and the fixture `populate()` were removed.
pub const PageRates = struct {
    const Self = @This();

    slots: []?model_rate_batch.ModelRate,
    has_default: bool,

    pub fn load(
        alloc: std.mem.Allocator,
        conn: *pg.Conn,
        entries: []const entries_state.Entry,
        meta: []const ?vault.SecretMetadata,
        default: ?tenant_provider.PlatformDefaultView,
    ) !Self {
        const count = entries.len + @intFromBool(default != null);
        const providers = try alloc.alloc([]const u8, count);
        defer alloc.free(providers);
        const models = try alloc.alloc([]const u8, count);
        defer alloc.free(models);

        for (entries, 0..) |e, i| {
            // A row whose credential is gone (or not yet backfilled) carries no
            // provider, so it has no catalogue identity to ask about. The empty
            // pair matches nothing and leaves the slot null — the same blank
            // cell that row already renders for every other metadata field.
            providers[i] = if (meta[i]) |m| (m.provider orelse "") else "";
            models[i] = e.model_id;
        }
        if (default) |d| {
            providers[count - 1] = d.provider;
            models[count - 1] = d.model;
        }

        const slots = try alloc.alloc(?model_rate_batch.ModelRate, count);
        errdefer alloc.free(slots);
        try model_rate_batch.loadRatesForPairs(conn, providers, models, slots);
        return .{ .slots = slots, .has_default = default != null };
    }

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.slots);
    }

    pub fn forDefault(self: Self) ?model_rate_batch.ModelRate {
        if (!self.has_default) return null;
        return self.slots[self.slots.len - 1];
    }
};

/// Build one wire row from an entry and its already-read projection.
///
/// No database handle and no tenant id: everything this needs was fetched in
/// bulk by `buildList`. That is the structural half of Invariant 5 — a function
/// with no connection cannot issue a query, so no future edit can quietly
/// reintroduce a per-row read here.
///
/// A missing credential (deleted out-of-band, or a row not yet backfilled)
/// degrades to an opaque custom_secret with no key, so the list still returns
/// 200 — mirroring `fleets/secret_list.zig`'s per-row resilience.
pub fn projectEntry(
    alloc: std.mem.Allocator,
    e: entries_state.Entry,
    active: bool,
    meta: ?vault.SecretMetadata,
    rate: ?model_rate_batch.ModelRate,
) !EntryView {
    const id = try alloc.dupe(u8, e.id);
    errdefer alloc.free(id);
    const model_id = try alloc.dupe(u8, e.model_id);
    errdefer alloc.free(model_id);
    const secret_ref = try alloc.dupe(u8, e.secret_ref);
    errdefer alloc.free(secret_ref);

    const m = meta orelse return .{
        .id = id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .kind = metadata.Kind.custom_secret.wire(),
        .has_key = false,
        .active = active,
        .created_at = e.created_at,
    };

    const provider = try dupeOpt(alloc, m.provider);
    errdefer if (provider) |v| alloc.free(v);
    const base_url = try dupeOpt(alloc, m.base_url);
    errdefer if (base_url) |v| alloc.free(v);

    return .{
        .id = id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .provider = provider,
        .kind = m.kind.wire(),
        .base_url = base_url,
        .has_key = m.has_key,
        .context_cap_tokens = if (rate) |r| r.context_cap_tokens else null,
        .input_nanos_per_mtok = if (rate) |r| r.input_nanos_per_mtok else null,
        .cached_input_nanos_per_mtok = if (rate) |r| r.cached_input_nanos_per_mtok else null,
        .output_nanos_per_mtok = if (rate) |r| r.output_nanos_per_mtok else null,
        .active = active,
        .created_at = e.created_at,
    };
}

fn dupeOpt(alloc: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try alloc.dupe(u8, v) else null;
}

pub fn freeView(alloc: std.mem.Allocator, v: EntryView) void {
    alloc.free(v.id);
    alloc.free(v.model_id);
    alloc.free(v.secret_ref);
    if (v.provider) |p| alloc.free(p);
    if (v.base_url) |b| alloc.free(b);
    // v.kind is a static @tagName slice — not owned, never freed.
}
