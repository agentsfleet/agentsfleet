//! Watcher polling helpers — XREADGROUP read loops + PEL drain.
//!
//! Extracted from `worker_watcher.zig` so that file stays under the 350-line
//! cap. The `Watcher` type itself lives in the parent module; these are free
//! functions that take `*Watcher` and reach into its `cfg` for Redis +
//! consumer-name + dispatch.
//!
//! Two-phase poll cycle (greptile P2 on PR #251):
//!
//!   1. Drain the consumer's PEL with `XREADGROUP ... 0` (non-blocking).
//!      Without this, a transient dispatch failure (e.g. OOM during
//!      `spawnZombieThread`'s `alloc.dupe`) leaves the message neither
//!      ACKed nor redeliverable — `>` only delivers new entries, and a
//!      worker restart resumes from the group cursor, not the PEL. The
//!      drain pass picks those orphans back up.
//!   2. Block on `XREADGROUP ... >` for new entries.
//!
//! Each phase processes whatever entries Redis returns; dispatch errors are
//! logged but never propagate (per-entry recovery, not loop-fatal).

const std = @import("std");
const watcher_mod = @import("worker_watcher.zig");
const control_stream = @import("../zombie/control_stream.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = std.log.scoped(.worker_watcher);

const block_ms = "5000";
const batch_count = "16";

/// Run one full poll cycle: PEL drain + new-message read. Called by
/// `Watcher.run` once per loop iteration.
pub fn pollOnce(watcher: *watcher_mod.Watcher) !void {
    pollWithId(watcher, "0", false) catch |err| {
        log.warn("watcher.pel_drain_fail err={s}", .{@errorName(err)});
    };
    try pollWithId(watcher, ">", true);
}

fn pollWithId(watcher: *watcher_mod.Watcher, last_id: []const u8, blocking: bool) !void {
    var argv: [12][]const u8 = undefined;
    var n: usize = 0;
    argv[n] = "XREADGROUP";
    n += 1;
    argv[n] = "GROUP";
    n += 1;
    argv[n] = control_stream.consumer_group;
    n += 1;
    argv[n] = watcher.cfg.consumer_name;
    n += 1;
    argv[n] = "COUNT";
    n += 1;
    argv[n] = batch_count;
    n += 1;
    if (blocking) {
        argv[n] = "BLOCK";
        n += 1;
        argv[n] = block_ms;
        n += 1;
    }
    argv[n] = "STREAMS";
    n += 1;
    argv[n] = control_stream.stream_key;
    n += 1;
    argv[n] = last_id;
    n += 1;

    var resp = try watcher.cfg.redis.command(argv[0..n]);
    defer resp.deinit(watcher.cfg.redis.alloc);

    const entries = navigateEntries(resp) catch |err| switch (err) {
        error.NoEntries => return,
        else => return err,
    };
    for (entries) |*entry_val| {
        watcher.processEntry(entry_val.*) catch |err| {
            log.err("watcher.entry_fail err={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{@errorName(err)});
        };
    }
}

/// Navigate the XREADGROUP response shape:
///   [["zombie:control", [[msg_id, [k, v, ...]], ...]]]
/// Returns the inner entry array, or `error.NoEntries` if Redis returned
/// nil (BLOCK timeout, empty PEL, or stream missing).
fn navigateEntries(resp: redis_protocol.RespValue) ![]redis_protocol.RespValue {
    if (resp != .array) return error.NoEntries;
    const top = resp.array orelse return error.NoEntries;
    if (top.len == 0) return error.NoEntries;
    if (top[0] != .array) return error.WatcherMalformedResp;
    const stream_tuple = top[0].array orelse return error.WatcherMalformedResp;
    if (stream_tuple.len != 2) return error.WatcherMalformedResp;
    if (stream_tuple[1] != .array) return error.WatcherMalformedResp;
    const entries = stream_tuple[1].array orelse return error.NoEntries;
    if (entries.len == 0) return error.NoEntries;
    return entries;
}
