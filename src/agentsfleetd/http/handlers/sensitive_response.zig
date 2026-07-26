//! One-time sensitive JSON response boundary.
//!
//! Count first, serialize into one exact-capacity block, synchronously write,
//! then erase the serialized bytes. A write failure closes the connection so a
//! truncated response can never be followed by another keepalive response.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const response_size = @import("../response_size.zig");
const metrics = @import("../../observability/metrics_sensitive_memory.zig");

/// This boundary serializes with std.json's defaults, and the size must be
/// measured under the same options it is written with — see response_size.zig.
const JSON_OPTIONS: std.json.Stringify.Options = .{};

pub fn writeJson(res: *httpz.Response, status: std.http.Status, value: anytype) void {
    eraseBuffered(res);
    res.clearWriter();

    const size = response_size.encoded(value, JSON_OPTIONS) catch {
        serializationFailed(res);
        return;
    };
    res.buffer.ensureTotalCapacityPrecise(size) catch {
        serializationFailed(res);
        return;
    };

    // Publish the exact region before formatting so an error after a partial
    // write still leaves every touched byte visible to serializationFailed.
    res.buffer.writer.end = size;
    var fixed = std.Io.Writer.fixed(res.buffer.writer.buffered());
    std.json.fmt(value, JSON_OPTIONS).format(&fixed) catch {
        serializationFailed(res);
        return;
    };
    if (fixed.buffered().len != size) {
        serializationFailed(res);
        return;
    }

    res.content_type = .JSON;
    res.status = @intFromEnum(status);
    res.write() catch {
        metrics.incResponseWriteFailure();
        res.conn.handover = .close;
    };
    eraseBuffered(res);
}

fn serializationFailed(res: *httpz.Response) void {
    eraseBuffered(res);
    res.clearWriter();
    common.writeJson(res, .internal_server_error, .{});
}

fn eraseBuffered(res: *httpz.Response) void {
    const bytes = res.buffer.writer.buffered();
    if (bytes.len == 0) return;
    std.crypto.secureZero(u8, bytes);
    metrics.recordResponseErased(bytes.len);
}
