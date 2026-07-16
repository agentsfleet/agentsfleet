//! The subscription set behind one workspace stream.
//!
//! Owns three things the detached stream thread needs and the request arena
//! cannot provide (the arena dies when the handler returns; the thread does
//! not):
//!
//!   1. A copy of the caller's principal, so each refresh tick can re-authorize
//!      THIS caller — membership revoked mid-stream must cost the caller its
//!      frames, and that check can never be shared with other viewers.
//!   2. The set of activity channels currently attached to the shared consumer.
//!   3. The fleet-set version it last synced to, so an unchanged tick costs one
//!      integer compare — no query, no diff, no allocation.
//!
//! The expensive half — enumerating the workspace's fleets — is NOT here: it
//! lives in the process-wide `FleetSetCache`, which runs it once per workspace
//! per cadence for every viewer at once.

const FanIn = @This();

alloc: std.mem.Allocator,
/// Borrowed: boot-owned, outlives every stream thread.
ctx: *common.Context,
/// Owned copy — the request arena that held the path param is gone by the time
/// the stream thread runs.
workspace_id: []u8,
/// Owned copy of the caller's identity. The tick re-authorizes with this, so a
/// revoked member is torn down even though the workspace's fleet set never
/// changed.
principal: OwnedPrincipal,
/// The shared consumer every attached channel feeds: ONE queue, ONE futex wait,
/// one memory budget — regardless of how many fleets are fanned in.
sub: *subscription_hub.Subscription,
/// Channel names currently attached to `sub`. Owned.
channels: std.ArrayList([]u8) = .empty,
/// The `FleetSetCache` version `channels` reflects. 0 = never synced.
synced_version: u64 = 0,

/// The caller's identity, duped out of the request arena so the stream thread
/// can re-authorize on every tick. Scopes are a bitset — no allocation.
const OwnedPrincipal = struct {
    mode: principal_mod.AuthMode,
    user_id: ?[]u8,
    tenant_id: ?[]u8,
    workspace_scope_id: ?[]u8,
    scopes: principal_mod.ScopeSet,

    fn dupe(alloc: std.mem.Allocator, p: common.AuthPrincipal) !OwnedPrincipal {
        const user_id = try dupeOptional(alloc, p.user_id);
        errdefer freeOptional(alloc, user_id);
        const tenant_id = try dupeOptional(alloc, p.tenant_id);
        errdefer freeOptional(alloc, tenant_id);
        const workspace_scope_id = try dupeOptional(alloc, p.workspace_scope_id);
        return .{
            .mode = p.mode,
            .user_id = user_id,
            .tenant_id = tenant_id,
            .workspace_scope_id = workspace_scope_id,
            .scopes = p.scopes,
        };
    }

    fn deinit(self: OwnedPrincipal, alloc: std.mem.Allocator) void {
        freeOptional(alloc, self.user_id);
        freeOptional(alloc, self.tenant_id);
        freeOptional(alloc, self.workspace_scope_id);
    }

    /// Rebuild the borrowed shape the authorization helpers take.
    fn view(self: OwnedPrincipal) common.AuthPrincipal {
        return .{
            .mode = self.mode,
            .user_id = self.user_id,
            .tenant_id = self.tenant_id,
            .workspace_scope_id = self.workspace_scope_id,
            .scopes = self.scopes,
        };
    }
};

pub const CreateError = error{ OutOfMemory, HubStopped };

/// Build the fan-in on the caller's thread. Nothing is subscribed yet — the
/// first `sync` attaches the workspace's fleets, so a stream that never starts
/// costs no wire traffic.
pub fn create(
    ctx: *common.Context,
    workspace_id: []const u8,
    caller: common.AuthPrincipal,
) CreateError!*FanIn {
    const alloc = ctx.alloc;
    const self = try alloc.create(FanIn);
    errdefer alloc.destroy(self);

    const ws = try alloc.dupe(u8, workspace_id);
    errdefer alloc.free(ws);

    const owned = OwnedPrincipal.dupe(alloc, caller) catch return error.OutOfMemory;
    errdefer owned.deinit(alloc);

    try ctx.fleet_sets.retain(workspace_id);
    errdefer ctx.fleet_sets.release(workspace_id);

    // The label is the workspace — it names the consumer in logs and is never a
    // channel key (a shared consumer has no channel of its own).
    const sub = try ctx.hub.createSharedConsumer(workspace_id);
    errdefer sub.unref();

    self.* = .{
        .alloc = alloc,
        .ctx = ctx,
        .workspace_id = ws,
        .principal = owned,
        .sub = sub,
    };
    return self;
}

/// Detach every channel, release the shared set, and free. Single owner: the
/// stream thread (or the spawn-failure path on the request thread) calls this
/// exactly once.
pub fn destroy(self: *FanIn) void {
    for (self.channels.items) |name| {
        self.ctx.hub.detachChannel(self.sub, name);
        self.alloc.free(name);
    }
    self.channels.deinit(self.alloc);
    self.sub.unref();
    self.ctx.fleet_sets.release(self.workspace_id);
    self.principal.deinit(self.alloc);
    self.alloc.free(self.workspace_id);
    const alloc = self.alloc;
    self.* = undefined;
    alloc.destroy(self);
}

/// A set adjustment: how many channels were attached and detached this tick.
pub const Delta = struct { added: usize = 0, removed: usize = 0 };

/// What one refresh tick concluded.
pub const SyncResult = union(enum) {
    /// The attached set already matches the workspace's fleet set.
    unchanged,
    /// Channels were attached and/or detached.
    changed: Delta,
    /// The caller may no longer read this workspace — the stream must close.
    /// Every channel is detached before this is returned, so no frame can be
    /// delivered to a revoked caller.
    revoked,
    /// The database was unreachable this tick. The current set keeps serving;
    /// the next tick retries. A transient DB blip must not kill live streams.
    deferred,
};

/// Re-authorize the caller, then align the attached channel set with the
/// workspace's fleets.
///
/// Authorization is per-caller and runs EVERY tick (one indexed point lookup).
/// Enumeration is per-workspace and shared: this tick asks the cache to refresh
/// only if the set is stale, and exactly one viewer's tick across the whole
/// process actually runs that query.
pub fn sync(self: *FanIn, now_ms: i64) SyncResult {
    const conn = self.ctx.pool.acquire() catch return .deferred;
    defer self.ctx.pool.release(conn);

    // The security boundary, re-checked on every tick and never cached — but the
    // authorize-ONLY variant: a single indexed point lookup, no RLS `set_config`
    // write. The tenant context the enumeration needs is set by the cache's own
    // `enumerate` (against the workspace's OWNER tenant), so writing it here too
    // would be a wasted second round-trip on every 10 s tick of every stream.
    if (!common.authorizeWorkspace(conn, self.principal.view(), self.workspace_id)) {
        self.detachAll();
        return .revoked;
    }

    self.ctx.fleet_sets.refreshIfStale(conn, self.workspace_id, now_ms);

    const current = self.ctx.fleet_sets.version(self.workspace_id);
    switch (compareVersion(current, self.synced_version)) {
        .deferred => return .deferred,
        .unchanged => return .unchanged,
        .changed => {},
    }

    const snap = (self.ctx.fleet_sets.snapshot(self.workspace_id) catch return .deferred) orelse
        return .unchanged;
    defer snap.deinit(self.alloc);

    const delta = self.applySet(snap.fleet_ids) catch return .deferred;
    self.synced_version = snap.version;
    if (delta.added == 0 and delta.removed == 0) return .unchanged;
    return .{ .changed = delta };
}

const VersionComparison = enum { deferred, unchanged, changed };

fn compareVersion(current: u64, synced: u64) VersionComparison {
    if (current == 0) return .deferred;
    if (current == synced) return .unchanged;
    return .changed;
}

/// Attach the channels for fleets we do not hold, detach the ones that are
/// gone. Detach first: a fleet that disappeared must stop delivering even if a
/// later attach fails.
fn applySet(self: *FanIn, fleet_ids: []const []const u8) error{OutOfMemory}!Delta {
    var delta: Delta = .{};

    var i: usize = 0;
    while (i < self.channels.items.len) {
        const name = self.channels.items[i];
        if (channelInSet(name, fleet_ids)) {
            i += 1;
            continue;
        }
        self.ctx.hub.detachChannel(self.sub, name);
        self.alloc.free(name);
        _ = self.channels.swapRemove(i);
        delta.removed += 1;
    }

    for (fleet_ids) |fleet_id| {
        var buf: [activity_channel.BUF_LEN]u8 = undefined;
        const name = activity_channel.format(&buf, fleet_id) catch {
            log.warn("workspace_stream_channel_too_long", .{
                .workspace_id = self.workspace_id,
                .fleet_id = fleet_id,
            });
            continue;
        };
        if (self.holdsChannel(name)) continue;

        const owned = try self.alloc.dupe(u8, name);
        errdefer self.alloc.free(owned);
        try self.channels.append(self.alloc, owned);
        errdefer _ = self.channels.pop();
        // HubStopped means the process is draining: leave the set as-is and let
        // the stream thread exit through the hub's close broadcast.
        self.ctx.hub.attachChannel(self.sub, owned) catch |err| switch (err) {
            error.OutOfMemory => return error.OutOfMemory,
            error.HubStopped => {
                _ = self.channels.pop();
                self.alloc.free(owned);
                return delta;
            },
        };
        delta.added += 1;
    }
    return delta;
}

/// Drop every channel — used on revocation and on teardown. The consumer handle
/// survives (its owner frees it), so a racing reader push cannot dangle.
fn detachAll(self: *FanIn) void {
    for (self.channels.items) |name| {
        self.ctx.hub.detachChannel(self.sub, name);
        self.alloc.free(name);
    }
    self.channels.clearRetainingCapacity();
    self.synced_version = 0;
}

fn holdsChannel(self: *FanIn, name: []const u8) bool {
    for (self.channels.items) |held| {
        if (std.mem.eql(u8, held, name)) return true;
    }
    return false;
}

fn channelInSet(channel_name: []const u8, fleet_ids: []const []const u8) bool {
    const fleet_id = activity_channel.fleetId(channel_name) orelse return false;
    for (fleet_ids) |id| {
        if (std.mem.eql(u8, id, fleet_id)) return true;
    }
    return false;
}

/// Live fan-in size — the operator-visible "how many fleets is this connection
/// watching" number, and the test's proof that the fan-in tracks the set.
pub fn channelCount(self: *const FanIn) usize {
    return self.channels.items.len;
}

/// Borrowed fleet ids for the current channel set. Caller owns only the slice
/// list; each id points into `self.channels`.
pub fn fleetIdList(self: *const FanIn, alloc: std.mem.Allocator) error{OutOfMemory}![][]const u8 {
    const ids = try alloc.alloc([]const u8, self.channels.items.len);
    errdefer alloc.free(ids);
    for (self.channels.items, 0..) |name, i| {
        // Populated only by `activity_channel.format` in `applySet`.
        ids[i] = activity_channel.fleetId(name) orelse unreachable;
    }
    return ids;
}

const std = @import("std");
const common = @import("../common.zig");
const principal_mod = @import("../../../auth/principal.zig");
const subscription_hub = @import("../../../events/subscription_hub.zig");
const activity_channel = @import("../../../events/activity_channel.zig");
const logging = @import("log");
const log = logging.scoped(.http_workspace_events_stream);

fn dupeOptional(alloc: std.mem.Allocator, maybe: ?[]const u8) !?[]u8 {
    const value = maybe orelse return null;
    return try alloc.dupe(u8, value);
}

fn freeOptional(alloc: std.mem.Allocator, maybe: ?[]u8) void {
    if (maybe) |value| alloc.free(value);
}

test "fan-in defers while the shared fleet set has never completed enumeration" {
    try std.testing.expectEqual(VersionComparison.deferred, compareVersion(0, 0));
    try std.testing.expectEqual(VersionComparison.changed, compareVersion(1, 0));
    try std.testing.expectEqual(VersionComparison.unchanged, compareVersion(1, 1));
}
