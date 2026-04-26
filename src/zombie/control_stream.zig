//! Worker control plane — fleet-wide Redis stream that tells watchers when
//! to spawn, cancel, or reconfigure per-zombie threads. Distinct from the
//! per-zombie data plane (`zombie:{id}:events`) which carries actual work.
//!
//! Protocol: Redis stream `zombie:control` with consumer group `zombie_workers`.
//! Each entry is a flat key/value list with `type=...` plus variant-specific
//! fields. Multi-tenant routing rides in the payload (`workspace_id`/`zombie_id`),
//! not the stream key — RLS at the PG layer is the tenant boundary.

const std = @import("std");
const redis_client = @import("../queue/redis_client.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = std.log.scoped(.control_stream);

pub const stream_key = "zombie:control";
pub const consumer_group = "zombie_workers";

/// Bound on stream length — control plane is low-volume (install / kill / patch
/// only), 1k entries is plenty of replay window without unbounded growth.
const stream_maxlen = "1000";

pub const MessageType = enum {
    zombie_created,
    zombie_status_changed,
    zombie_config_changed,
    worker_drain_request,

    pub fn toSlice(self: MessageType) []const u8 {
        return switch (self) {
            .zombie_created => "zombie_created",
            .zombie_status_changed => "zombie_status_changed",
            .zombie_config_changed => "zombie_config_changed",
            .worker_drain_request => "worker_drain_request",
        };
    }

    pub fn fromSlice(s: []const u8) ?MessageType {
        if (std.mem.eql(u8, s, "zombie_created")) return .zombie_created;
        if (std.mem.eql(u8, s, "zombie_status_changed")) return .zombie_status_changed;
        if (std.mem.eql(u8, s, "zombie_config_changed")) return .zombie_config_changed;
        if (std.mem.eql(u8, s, "worker_drain_request")) return .worker_drain_request;
        return null;
    }
};

pub const ZombieStatus = enum {
    active,
    killed,
    paused,

    pub fn toSlice(self: ZombieStatus) []const u8 {
        return switch (self) {
            .active => "active",
            .killed => "killed",
            .paused => "paused",
        };
    }

    pub fn fromSlice(s: []const u8) ?ZombieStatus {
        if (std.mem.eql(u8, s, "active")) return .active;
        if (std.mem.eql(u8, s, "killed")) return .killed;
        if (std.mem.eql(u8, s, "paused")) return .paused;
        return null;
    }
};

pub const ControlMessage = union(MessageType) {
    zombie_created: struct {
        zombie_id: []const u8,
        workspace_id: []const u8,
    },
    zombie_status_changed: struct {
        zombie_id: []const u8,
        status: ZombieStatus,
    },
    zombie_config_changed: struct {
        zombie_id: []const u8,
        config_revision: i64,
    },
    worker_drain_request: struct {
        reason: ?[]const u8 = null,
    },
};

/// Result of decoding a single XREADGROUP entry. Owns its backing strings;
/// the `ControlMessage` slices borrow from `owned_fields`.
///
/// Caller passes the same allocator to `deinit` that `decodeEntry` received —
/// `Decoded` deliberately does not store the allocator, since instances are
/// transient (one per stream entry) and live only inside the watcher's
/// dispatch loop. See ZIG_RULES.md "Allocator Ownership in Structs".
pub const Decoded = struct {
    message_id: []u8,
    message: ControlMessage,
    owned_fields: [][]u8,

    pub fn deinit(self: *Decoded, alloc: std.mem.Allocator) void {
        alloc.free(self.message_id);
        for (self.owned_fields) |s| alloc.free(s);
        alloc.free(self.owned_fields);
    }
};

/// Idempotent per-zombie `XGROUP CREATE MKSTREAM zombie:{id}:events` with
/// BUSYGROUP-as-success. Called from `innerCreateZombie` (install path) AND
/// `worker_watcher.spawnZombieThread` (bootstrap self-heal: orphan rows from
/// a failed install-time XADD recover at next worker boot, no reconcile job).
pub fn ensureZombieEventsGroup(client: *redis_client.Client, zombie_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id});
    var resp = try client.commandAllowError(&.{ "XGROUP", "CREATE", key, consumer_group, "0", "MKSTREAM" });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.ZombieEventsGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return;
            log.err("zombie_events.group_create_fail err={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{msg});
            return error.ZombieEventsGroupCreateFailed;
        },
        else => return error.ZombieEventsGroupCreateFailed,
    }
}

/// Idempotent `XGROUP CREATE MKSTREAM zombie:control zombie_workers 0`.
/// Safe to call on every worker start — `BUSYGROUP` is treated as success.
/// Starts at id `0` so a fresh group reads any messages already on the
/// stream; subsequent replicas joining the same group resume from the
/// group's ACK pointer (Redis-managed state).
pub fn ensureControlGroup(client: *redis_client.Client) !void {
    var resp = try client.commandAllowError(&.{
        "XGROUP", "CREATE", stream_key, consumer_group, "0", "MKSTREAM",
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.ControlGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return;
            log.err("control.group_create_fail err={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{msg});
            return error.ControlGroupCreateFailed;
        },
        else => return error.ControlGroupCreateFailed,
    }
}

/// `XADD zombie:control MAXLEN ~ 1000 * type=... <fields...>`. Synchronous —
/// callers (API mutation paths) must publish BEFORE returning to the caller
/// so the watcher sees the signal by the time the HTTP response lands.
pub fn publish(client: *redis_client.Client, msg: ControlMessage) !void {
    var argv: [16][]const u8 = undefined;
    var n: usize = 0;
    argv[n] = "XADD";
    n += 1;
    argv[n] = stream_key;
    n += 1;
    argv[n] = "MAXLEN";
    n += 1;
    argv[n] = "~";
    n += 1;
    argv[n] = stream_maxlen;
    n += 1;
    argv[n] = "*";
    n += 1;
    argv[n] = "type";
    n += 1;

    const tag: MessageType = std.meta.activeTag(msg);
    argv[n] = tag.toSlice();
    n += 1;

    var revision_buf: [24]u8 = undefined;
    n = appendVariantFields(&argv, n, msg, &revision_buf) catch |err| {
        log.err("control.publish_encode_fail type={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{tag.toSlice()});
        return err;
    };

    std.debug.assert(n <= argv.len);

    var resp = try client.command(argv[0..n]);
    defer resp.deinit(client.alloc);
    switch (resp) {
        .bulk => |v| if (v == null) {
            log.err("control.xadd_fail type={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{tag.toSlice()});
            return error.ControlXaddFailed;
        },
        else => {
            log.err("control.xadd_fail type={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{tag.toSlice()});
            return error.ControlXaddFailed;
        },
    }
    log.debug("control.xadd type={s}", .{tag.toSlice()});
}

fn appendVariantFields(
    argv: *[16][]const u8,
    start: usize,
    msg: ControlMessage,
    revision_buf: *[24]u8,
) !usize {
    var n = start;
    switch (msg) {
        .zombie_created => |m| {
            argv[n] = "zombie_id";
            argv[n + 1] = m.zombie_id;
            argv[n + 2] = "workspace_id";
            argv[n + 3] = m.workspace_id;
            n += 4;
        },
        .zombie_status_changed => |m| {
            argv[n] = "zombie_id";
            argv[n + 1] = m.zombie_id;
            argv[n + 2] = "status";
            argv[n + 3] = m.status.toSlice();
            n += 4;
        },
        .zombie_config_changed => |m| {
            const rev_str = try std.fmt.bufPrint(revision_buf, "{d}", .{m.config_revision});
            argv[n] = "zombie_id";
            argv[n + 1] = m.zombie_id;
            argv[n + 2] = "config_revision";
            argv[n + 3] = rev_str;
            n += 4;
        },
        .worker_drain_request => |m| {
            if (m.reason) |r| {
                argv[n] = "reason";
                argv[n + 1] = r;
                n += 2;
            }
        },
    }
    return n;
}

/// Decode a single XREADGROUP stream entry. `fields` is the flat key/value
/// alternating list (RESP array of bulk strings). Returns owned heap memory;
/// caller must call `decoded.deinit(alloc)`.
pub fn decodeEntry(
    alloc: std.mem.Allocator,
    msg_id: []const u8,
    fields: []const redis_protocol.RespValue,
) !Decoded {
    if (fields.len % 2 != 0) return error.ControlDecodeMalformed;

    var msg_type: ?MessageType = null;
    var zombie_id: ?[]const u8 = null;
    var workspace_id: ?[]const u8 = null;
    var status: ?ZombieStatus = null;
    var config_revision: i64 = 0;
    var reason: ?[]const u8 = null;

    var i: usize = 0;
    while (i < fields.len) : (i += 2) {
        const k = redis_protocol.valueAsString(fields[i]) orelse continue;
        const v = redis_protocol.valueAsString(fields[i + 1]) orelse continue;
        if (std.mem.eql(u8, k, "type")) {
            msg_type = MessageType.fromSlice(v);
        } else if (std.mem.eql(u8, k, "zombie_id")) {
            zombie_id = v;
        } else if (std.mem.eql(u8, k, "workspace_id")) {
            workspace_id = v;
        } else if (std.mem.eql(u8, k, "status")) {
            status = ZombieStatus.fromSlice(v);
        } else if (std.mem.eql(u8, k, "config_revision")) {
            config_revision = std.fmt.parseInt(i64, v, 10) catch 0;
        } else if (std.mem.eql(u8, k, "reason")) {
            reason = v;
        }
    }

    const t = msg_type orelse return error.ControlDecodeUnknownType;

    var owned: std.ArrayList([]u8) = .{};
    errdefer {
        for (owned.items) |s| alloc.free(s);
        owned.deinit(alloc);
    }

    const id_owned = try alloc.dupe(u8, msg_id);
    errdefer alloc.free(id_owned);

    const message = try buildOwnedMessage(alloc, &owned, t, .{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .status = status,
        .config_revision = config_revision,
        .reason = reason,
    });

    return .{
        .message_id = id_owned,
        .message = message,
        .owned_fields = try owned.toOwnedSlice(alloc),
    };
}

const DecodeFields = struct {
    zombie_id: ?[]const u8,
    workspace_id: ?[]const u8,
    status: ?ZombieStatus,
    config_revision: i64,
    reason: ?[]const u8,
};

fn buildOwnedMessage(
    alloc: std.mem.Allocator,
    owned: *std.ArrayList([]u8),
    t: MessageType,
    f: DecodeFields,
) !ControlMessage {
    return switch (t) {
        .zombie_created => blk: {
            const zid = f.zombie_id orelse return error.ControlDecodeMissingField;
            const wid = f.workspace_id orelse return error.ControlDecodeMissingField;
            const zid_owned = try alloc.dupe(u8, zid);
            try owned.append(alloc, zid_owned);
            const wid_owned = try alloc.dupe(u8, wid);
            try owned.append(alloc, wid_owned);
            break :blk .{ .zombie_created = .{ .zombie_id = zid_owned, .workspace_id = wid_owned } };
        },
        .zombie_status_changed => blk: {
            const zid = f.zombie_id orelse return error.ControlDecodeMissingField;
            const st = f.status orelse return error.ControlDecodeMissingField;
            const zid_owned = try alloc.dupe(u8, zid);
            try owned.append(alloc, zid_owned);
            break :blk .{ .zombie_status_changed = .{ .zombie_id = zid_owned, .status = st } };
        },
        .zombie_config_changed => blk: {
            const zid = f.zombie_id orelse return error.ControlDecodeMissingField;
            const zid_owned = try alloc.dupe(u8, zid);
            try owned.append(alloc, zid_owned);
            break :blk .{ .zombie_config_changed = .{ .zombie_id = zid_owned, .config_revision = f.config_revision } };
        },
        .worker_drain_request => blk: {
            var reason_owned: ?[]const u8 = null;
            if (f.reason) |r| {
                const r_owned = try alloc.dupe(u8, r);
                try owned.append(alloc, r_owned);
                reason_owned = r_owned;
            }
            break :blk .{ .worker_drain_request = .{ .reason = reason_owned } };
        },
    };
}
