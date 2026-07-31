//! Bounded JSON Web Key Set (JWKS) transport, split from jwks.zig by concern
//! (and the file cap): one blocking GET whose body read rejects at a named
//! byte cap. std + common only — same portability wall as the rest of
//! `src/agentsfleetd/auth/`.

const std = @import("std");
const common = @import("common");

/// Upper bound for a JWKS document. Real key sets are a few KiB; a response
/// past this is rejected at the cap instead of accumulated.
pub const JWKS_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DRAIN_CHUNK_BYTES: usize = 4096;
const HEAD_BUFFER_BYTES: usize = 8 * 1024;
/// Identity providers commonly front the key set with one hop; three covers
/// chained CDN redirects without following forever.
const MAX_REDIRECTS = 3;

pub const FetchError = error{ OutOfMemory, FetchFailed, ResponseTooLarge };

/// Fetch `url` and return the response body; caller must free. Any transport
/// fault, non-200 status, or body larger than `JWKS_MAX_RESPONSE_BYTES` is an
/// error — the partial accumulation is freed on every failure path.
pub fn fetchCapped(alloc: std.mem.Allocator, url: []const u8) FetchError![]u8 {
    var client: std.http.Client = .{ .allocator = alloc, .io = common.globalIo() };
    defer client.deinit();

    const uri = std.Uri.parse(url) catch return FetchError.FetchFailed;
    var req = client.request(.GET, uri, .{
        .redirect_behavior = @enumFromInt(MAX_REDIRECTS),
    }) catch return FetchError.FetchFailed;
    defer req.deinit();
    req.sendBodiless() catch return FetchError.FetchFailed;
    var head_buffer: [HEAD_BUFFER_BYTES]u8 = undefined;
    var response = req.receiveHead(&head_buffer) catch return FetchError.FetchFailed;
    if (response.head.status != .ok) return FetchError.FetchFailed;

    // The defer covers every error path below, where the partially-written
    // body would otherwise be abandoned; after a successful toOwnedSlice it
    // frees an empty writer (no-op).
    var aw: std.Io.Writer.Allocating = .init(alloc);
    defer aw.deinit();
    var transfer_buffer: [DRAIN_CHUNK_BYTES]u8 = undefined;
    const reader = response.reader(&transfer_buffer);
    var total: usize = 0;
    var chunk: [DRAIN_CHUNK_BYTES]u8 = undefined;
    while (true) {
        const n = reader.readSliceShort(&chunk) catch return FetchError.FetchFailed;
        if (n == 0) break;
        total += n;
        if (total > JWKS_MAX_RESPONSE_BYTES) return FetchError.ResponseTooLarge;
        aw.writer.writeAll(chunk[0..n]) catch return FetchError.OutOfMemory;
    }
    return aw.toOwnedSlice() catch return FetchError.OutOfMemory;
}
