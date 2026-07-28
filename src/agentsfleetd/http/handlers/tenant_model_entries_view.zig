//! GET /v1/tenants/me/models — list-view construction.
//!
//! Joins each `core.tenant_model_entries` row to its secret's non-secret
//! metadata (provider/kind/base_url/has_key), computes `active` against the
//! tenant's current `core.tenant_model_selection` row, and resolves
//! context/rates from the model library cache when known. Pure read — the
//! "every active selection has a matching entry" invariant is guaranteed at
//! activation-write time (tenant_provider.zig's ensureEntryForSelection), never
//! patched up here. Split out of tenant_model_entries.zig (the 4-endpoint
//! handler) per RULE FLL.
//!
//! THIS READ DECRYPTS NOTHING (the never-decrypt invariant). Every field it
//! displays is metadata that now lives in the `meta_*` columns
//! (`schema/036_vault_secret_metadata.sql`), written beside the ciphertext at
//! store time. One batch query answers the whole page.
//!
//! What that replaced: `projectEntry` called `secret_probe.loadTenantSecretJson`
//! per row, and each call resolved the primary workspace AND opened an AES-GCM
//! envelope. A 100-row page cost ~200 statements and 100 decryptions to render
//! a view whose every field is shown to any authorized caller. Now it costs one
//! workspace lookup and one metadata query, and no ciphertext is loaded at all —
//! so there is no plaintext to leak, mishandle, or forget to zero.

const std = @import("std");
const pg = @import("pg");

const entries_state = @import("../../state/tenant_model_entries.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const secret_probe = @import("../../state/secret_probe.zig");
const vault = @import("../../state/vault.zig");
const pagination = @import("../pagination.zig");
const projection = @import("tenant_model_entries_projection.zig");
const ReadScope = @import("../../observability/library_read_scope.zig");

/// Re-exported so callers keep one import for the page's shape. The wire row
/// and the function that builds it belong together, and they live next door.
pub const EntryView = projection.EntryView;
const freeView = projection.freeView;
const PageRates = projection.PageRates;

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

/// The cursor payload for this page. Field order IS the canonical JSON key
/// order (`http/pagination.zig`), so reordering these fields invalidates every
/// cursor already in flight — bump `CURSOR_VERSION` if that ever happens.
///
/// It carries `tenant_uuid` and `limit` as well as the sort key: a cursor is
/// bound to the query that produced it, so replaying one against a different
/// tenant or a different page size is rejected rather than silently answered.
pub const Cursor = struct {
    v: u8 = pagination.CURSOR_VERSION,
    created_at: i64,
    id: []const u8,
    tenant_uuid: []const u8,
    limit: u32,
};

pub const ListResult = struct {
    rows: []EntryView,
    /// Opaque cursor for the next page, or null on the last one. Owned by the
    /// same allocator as `rows`.
    next_cursor: ?[]const u8 = null,
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
        if (self.next_cursor) |c| alloc.free(c);
        if (self.platform_default) |*dv| dv.deinit(alloc);
    }
};

/// Caller owns the result and must call `.deinit(alloc)`. Activation
/// (tenant_provider.zig) guarantees the selection always has a matching
/// entry row, so no synthesize-on-read exists here.
///
/// Statement budget, whatever the page size: one selection read, one entry
/// page, one workspace resolve, one metadata batch, one platform default, one
/// rate batch. Decryptions: zero. Both batches are set-oriented, so the count
/// is independent of `limit` — that independence is the property §3 pins, not
/// the number itself.
///
/// `after` is the decoded boundary from the caller's cursor, already checked
/// against the authenticated tenant and the requested limit — this function
/// trusts it, because only the handler can perform that comparison.
pub fn buildList(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    limit: u32,
    after: ?entries_state.PageStart,
    scope: ?*ReadScope,
) !ListResult {
    var selection = try tenant_provider.activeSelfManagedRef(alloc, conn, tenant_id);
    defer if (selection) |*s| s.deinit(alloc);

    const page = try entries_state.listPage(alloc, conn, tenant_id, limit, after);
    const entries = page.rows;
    defer entries_state.deinitEntryList(entries, alloc);

    // One workspace resolve for the whole page, not one per row: the credentials
    // a tenant's entries reference all live in its primary workspace.
    const ws_id = try secret_probe.resolvePrimaryWorkspace(alloc, conn, tenant_id);
    defer alloc.free(ws_id);

    // Positional, one slot per entry — deliberately NOT deduplicated. One
    // credential can back several model rows, so an earlier draft collected the
    // distinct set and then scanned it per row to match each entry back. That
    // cost two O(n²) passes and two helpers to save a few repeated key names in
    // one query parameter. `key_name = ANY($2)` is indifferent to duplicates, so
    // asking positionally makes `meta[i]` belong to `entries[i]` by construction
    // and deletes the matching problem instead of solving it.
    const refs = try alloc.alloc([]const u8, entries.len);
    defer alloc.free(refs);
    for (entries, 0..) |e, i| refs[i] = e.secret_ref;

    // The three reads above are this page's SQL up to here; the metadata batch
    // is timed separately below because it is the stage that USED to decrypt.
    if (scope) |s| s.endStage(.sql);

    const meta = try alloc.alloc(?vault.SecretMetadata, entries.len);
    defer alloc.free(meta);
    try vault.loadMetadata(alloc, conn, ws_id, refs, meta);
    defer vault.freeMetadata(alloc, meta);

    // `secret_project` is kept deliberately, with a narrowed meaning: it now
    // times presence resolution and projection rather than per-row decryption.
    // Deleting the label once the decryption left would make a regression that
    // reintroduces per-row envelope opens show up as a stage that silently
    // REAPPEARS; keeping it means such a regression shows up as this stage
    // suddenly decrypting, which is the thing an assertion can catch.
    if (scope) |s| s.endStage(.secret_project);

    // Resolved BEFORE the rates, so the default's `(provider, model)` rides in
    // the same statement as the page's rather than costing a second one.
    //
    // Sequential reuse of `conn` is safe: every query above
    // (`activeSelfManagedRef`, `list`, `resolvePrimaryWorkspace`,
    // `loadMetadata`) fully drains its own result set before returning —
    // mirrors `fleets/secret_list.zig`. A failure reading the default degrades
    // to "no default known" rather than failing the list — the posture the
    // boolean always had.
    var source_default = tenant_provider.platformDefaultView(alloc, conn) catch null;
    errdefer if (source_default) |*d| d.deinit(alloc);

    var rates = try PageRates.load(alloc, conn, entries, meta, source_default);
    defer rates.deinit(alloc);
    // The default read and the rate batch are SQL too, so they fold into the
    // same cell — the stage table attributes the request's own time, and two
    // separate SQL spans of one read are one stage's cost.
    if (scope) |s| s.endStage(.sql);

    var views: std.ArrayList(EntryView) = .empty;
    errdefer {
        for (views.items) |v| freeView(alloc, v);
        views.deinit(alloc);
    }
    for (entries, 0..) |e, i| {
        const active = if (selection) |s|
            std.mem.eql(u8, e.secret_ref, s.secret_ref) and std.mem.eql(u8, e.model_id, s.model)
        else
            false;
        const view = try projection.projectEntry(alloc, e, active, meta[i], rates.slots[i]);
        errdefer freeView(alloc, view);
        try views.append(alloc, view);
    }

    // Takes ownership of the resolved default's strings — no dupe. The source
    // is nulled out to disarm ITS errdefer, so from here exactly one owner
    // (the errdefer below) can free `provider`/`model`; leaving both armed
    // double-freed them when a later step (`pagination.encode`,
    // `views.toOwnedSlice`) failed.
    var platform_default: ?PlatformDefaultView = if (source_default) |d| .{
        .provider = d.provider,
        .model = d.model,
        .context_cap_tokens = d.context_cap_tokens,
        .input_nanos_per_mtok = if (rates.forDefault()) |r| r.input_nanos_per_mtok else null,
        .cached_input_nanos_per_mtok = if (rates.forDefault()) |r| r.cached_input_nanos_per_mtok else null,
        .output_nanos_per_mtok = if (rates.forDefault()) |r| r.output_nanos_per_mtok else null,
    } else null;
    source_default = null;
    errdefer if (platform_default) |*dv| dv.deinit(alloc);

    // The cursor is built from the LAST ENTRY ROW, not from the last view: the
    // seek predicate compares against `core.tenant_model_entries` columns, and
    // the view's fields are a projection that may not round-trip them.
    const next_cursor: ?[]const u8 = if (page.has_more and entries.len > 0)
        try pagination.encode(alloc, Cursor, .{
            .created_at = entries[entries.len - 1].created_at,
            .id = entries[entries.len - 1].id,
            .tenant_uuid = tenant_id,
            .limit = limit,
        })
    else
        null;
    errdefer if (next_cursor) |c| alloc.free(c);

    if (scope) |s| s.endStage(.map);

    return .{
        .rows = try views.toOwnedSlice(alloc),
        .next_cursor = next_cursor,
        .platform_default_available = platform_default != null,
        .platform_default = platform_default,
    };
}
