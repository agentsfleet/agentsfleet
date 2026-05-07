//! Per-RPC-call writer that emits ProgressFrame notifications onto the
//! same Unix socket carrying the StartStage request. The transport server
//! constructs one of these for every inbound frame, hands it to the
//! handler, and the handler threads it through to runner.execute. The
//! NullClaw observer/stream-callback adapters in runner_progress.zig
//! call `write` synchronously from inside agent.runSingle().
//!
//! Wire shape: each ProgressFrame is encoded as a JSON-RPC notification
//! that shares the StartStage request id, then flushed through
//! protocol.writeFrameToFd. The worker side reads it in
//! transport.Client.sendRequestStreaming and dispatches to its
//! ProgressEmitter before continuing the read loop.

const ProgressWriter = @This();

fd: std.posix.socket_t,
request_id: u64,
alloc: std.mem.Allocator,

/// Encode `frame` as a JSON-RPC progress notification carrying this
/// writer's request_id and write it to the connection. Errors are
/// swallowed at debug level — progress emission is best-effort and must
/// never abort agent execution.
pub fn write(self: *const ProgressWriter, frame: progress_callbacks.ProgressFrame) void {
    const payload = progress_callbacks.encodeProgress(self.alloc, self.request_id, frame) catch |err| {
        log.debug("encode_failed", .{ .err = @errorName(err) });
        return;
    };
    defer self.alloc.free(payload);
    protocol.writeFrameToFd(self.fd, payload) catch |err| {
        log.debug("write_failed", .{ .err = @errorName(err) });
    };
}

const std = @import("std");
const logging = @import("log");
const progress_callbacks = @import("progress_callbacks.zig");
const protocol = @import("protocol.zig");
const log = logging.scoped(.progress_writer);
