//! GET /v1/tenants/me/models — list-view construction.
//!
//! Joins each `core.tenant_model_entries` row to its secret's non-secret
//! metadata (provider/kind/base_url/has_key) via `secret_metadata.project`,
//! computes `active` against the tenant's current `core.tenant_model_selection`
//! row, and resolves context/rates from the model library cache when known.
//! Pure read — the "every active selection has a matching entry"
//! invariant is guaranteed at activation-write time (tenant_provider.zig's
//! ensureEntryForSelection), never patched up here. Split out of
//! tenant_model_entries.zig (the 4-endpoint handler) per RULE FLL.

const std = @import("std");
const pg = @import("pg");

const entries_state = @import("../../state/tenant_model_entries.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const secret_probe = @import("../../state/secret_probe.zig");
const secret_metadata = @import("fleets/secret_metadata.zig");
const model_rate_cache = @import("../../state/model_rate_cache.zig");

const S_API_KEY = "api_key";

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

pub const PlatformDefaultView = struct {
    const Self = @This();

    provider: []u8,
    model: []u8,
    context_cap_tokens: u32,
    input_nanos_per_mtok: ?i64 = null,
    cached_input_nanos_per_mtok: ?i64 = null,
    output_nanos_per_mtok: ?i64 = null,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.model);
    }
};

pub const ListResult = struct {
    rows: []EntryView,
    platform_default_available: bool,
    /// The active platform default's identity — the Models page renders the
    /// Default row's model/context from it. Omitted from the wire
    /// (`emit_null_optional_fields=false`) when no default is configured;
    /// `platform_default_available` derives from the same read, so the two
    /// can never disagree.
    platform_default: ?PlatformDefaultView = null,

    pub fn deinit(self: *ListResult, alloc: std.mem.Allocator) void {
        for (self.rows) |r| freeView(alloc, r);
        alloc.free(self.rows);
        if (self.platform_default) |*dv| dv.deinit(alloc);
    }
};

/// Caller owns the result and must call `.deinit(alloc)`. Fetches the active
/// selection and the entry list once each — a pure read. Activation
/// (tenant_provider.zig) guarantees the selection always has a matching
/// entry row, so no synthesize-on-read exists here.
pub fn buildList(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8) !ListResult {
    var selection = try tenant_provider.activeSelfManagedRef(alloc, conn, tenant_id);
    defer if (selection) |*s| s.deinit(alloc);

    const entries = try entries_state.list(alloc, conn, tenant_id);
    defer entries_state.deinitEntryList(entries, alloc);

    var views: std.ArrayList(EntryView) = .empty;
    errdefer {
        for (views.items) |v| freeView(alloc, v);
        views.deinit(alloc);
    }
    for (entries) |e| {
        const active = if (selection) |s|
            std.mem.eql(u8, e.secret_ref, s.secret_ref) and std.mem.eql(u8, e.model_id, s.model)
        else
            false;
        const view = try projectEntry(alloc, conn, tenant_id, e, active);
        errdefer freeView(alloc, view);
        try views.append(alloc, view);
    }

    // Sequential reuse of `conn` is safe: every query above (`list`,
    // `activeSelfManagedRef`, each `loadTenantSecretJson`) fully drains its
    // own result set before returning — mirrors `fleets/secret_list.zig`.
    // A failure reading the default degrades to "no default known" rather
    // than failing the list — the posture the boolean always had.
    var platform_default = platformDefaultView(alloc, conn) catch null;
    errdefer if (platform_default) |*dv| dv.deinit(alloc);

    return .{
        .rows = try views.toOwnedSlice(alloc),
        .platform_default_available = platform_default != null,
        .platform_default = platform_default,
    };
}

/// A vault load failure (secret deleted out-of-band, decrypt error) degrades
/// the row to an opaque custom_secret with no key — mirrors
/// `fleets/secret_list.zig`'s resilience so the list still returns 200.
fn projectEntry(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8, e: entries_state.Entry, active: bool) !EntryView {
    const id = try alloc.dupe(u8, e.id);
    errdefer alloc.free(id);
    const model_id = try alloc.dupe(u8, e.model_id);
    errdefer alloc.free(model_id);
    const secret_ref = try alloc.dupe(u8, e.secret_ref);
    errdefer alloc.free(secret_ref);

    var parsed = secret_probe.loadTenantSecretJson(alloc, conn, tenant_id, e.secret_ref) catch {
        return .{
            .id = id,
            .model_id = model_id,
            .secret_ref = secret_ref,
            .kind = secret_metadata.Kind.custom_secret.wire(),
            .has_key = false,
            .active = active,
            .created_at = e.created_at,
        };
    };
    defer parsed.deinit();

    const p = secret_metadata.project(parsed.value);
    const provider = try dupeOpt(alloc, p.provider);
    errdefer if (provider) |v| alloc.free(v);
    const base_url = try dupeOpt(alloc, p.base_url);
    errdefer if (base_url) |v| alloc.free(v);
    const rate = if (p.provider) |prov| lookupModelRate(prov, model_id) else null;

    return .{
        .id = id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .provider = provider,
        .kind = p.kind.wire(),
        .base_url = base_url,
        .has_key = hasNonEmptyApiKey(parsed.value),
        .context_cap_tokens = if (rate) |r| r.context_cap_tokens else null,
        .input_nanos_per_mtok = if (rate) |r| r.input_nanos_per_mtok else null,
        .cached_input_nanos_per_mtok = if (rate) |r| r.cached_input_nanos_per_mtok else null,
        .output_nanos_per_mtok = if (rate) |r| r.output_nanos_per_mtok else null,
        .active = active,
        .created_at = e.created_at,
    };
}

fn hasNonEmptyApiKey(value: std.json.Value) bool {
    if (value != .object) return false;
    const v = value.object.get(S_API_KEY) orelse return false;
    return v == .string and v.string.len > 0;
}

fn platformDefaultView(alloc: std.mem.Allocator, conn: *pg.Conn) !?PlatformDefaultView {
    const source = (try tenant_provider.platformDefaultView(alloc, conn)) orelse return null;
    const rate = lookupModelRate(source.provider, source.model);
    return .{
        .provider = source.provider,
        .model = source.model,
        .context_cap_tokens = source.context_cap_tokens,
        .input_nanos_per_mtok = if (rate) |r| r.input_nanos_per_mtok else null,
        .cached_input_nanos_per_mtok = if (rate) |r| r.cached_input_nanos_per_mtok else null,
        .output_nanos_per_mtok = if (rate) |r| r.output_nanos_per_mtok else null,
    };
}

fn lookupModelRate(provider: []const u8, model_id: []const u8) ?model_rate_cache.ModelRate {
    return model_rate_cache.lookup_model_rate(provider, model_id);
}

fn dupeOpt(alloc: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try alloc.dupe(u8, v) else null;
}

fn freeView(alloc: std.mem.Allocator, v: EntryView) void {
    alloc.free(v.id);
    alloc.free(v.model_id);
    alloc.free(v.secret_ref);
    if (v.provider) |p| alloc.free(p);
    if (v.base_url) |b| alloc.free(b);
    // v.kind is a static @tagName slice — not owned, never freed.
}
