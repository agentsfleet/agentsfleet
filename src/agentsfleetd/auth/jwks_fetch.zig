//! Bounded JSON Web Key Set (JWKS) transport, split from jwks.zig by concern
//! (and the file cap): one blocking GET that decodes the negotiated
//! content-encoding and rejects at a named cap on the DECODED bytes. std +
//! common only — same portability wall as the rest of
//! `src/agentsfleetd/auth/`.

const std = @import("std");
const common = @import("common");

/// Upper bound for a JWKS document, counted in DECOMPRESSED bytes — the size
/// the caller would actually receive. Real key sets are a few KiB. Capping the
/// decoded stream (not the wire) is what bounds a decompression bomb: a few KiB
/// of deflated zeroes inflates past any wire-byte limit.
pub const JWKS_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DRAIN_CHUNK_BYTES: usize = 4096;
const HEAD_BUFFER_BYTES: usize = 8 * 1024;
/// Sliding window the flate codec needs to reconstruct the stream. Sized from
/// the standard library's own constant, matching `std.http.Client.fetch`; it
/// bounds the decoder's history, never the output (that is the cap above).
const FLATE_WINDOW_BYTES: usize = std.compress.flate.max_window_len;
/// Identity providers commonly front the key set with one hop; three covers
/// chained CDN redirects without following forever.
const MAX_REDIRECTS = 3;

pub const FetchError = error{ OutOfMemory, FetchFailed, ResponseTooLarge };

/// Fetch `url` and return the DECODED response body; caller must free. Any
/// transport fault, non-200 status, or decompressed body larger than
/// `JWKS_MAX_RESPONSE_BYTES` is an error — the partial accumulation is freed on
/// every failure path. A cap rejection stays `ResponseTooLarge` and a transport
/// fault stays `FetchFailed`: the two describe different operator situations
/// (implausible provider response vs. unreachable provider) and never collapse.
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

    // This client advertises gzip/deflate/identity, and providers honour it —
    // Clerk answers `content-encoding: gzip`. `reader` hands back the ENCODED
    // bytes; only `readerDecompressing` decodes them, which is what
    // `std.http.Client.fetch` does internally. Reading raw here fed the JSON
    // parser gzip bytes and took every token verification down with it.
    const decompress_buffer: []u8 = switch (response.head.content_encoding) {
        .identity => &.{},
        .gzip, .deflate => alloc.alloc(u8, FLATE_WINDOW_BYTES) catch
            return FetchError.OutOfMemory,
        // `receiveHead` already refuses an encoding this client never
        // advertised, so these cannot reach us from a conforming transport.
        // Refuse rather than size a window for an unreachable branch.
        .zstd, .compress => return FetchError.FetchFailed,
    };
    defer alloc.free(decompress_buffer);

    // The defer covers every error path below, where the partially-written
    // body would otherwise be abandoned; after a successful toOwnedSlice it
    // frees an empty writer (no-op).
    var aw: std.Io.Writer.Allocating = .init(alloc);
    defer aw.deinit();
    var transfer_buffer: [DRAIN_CHUNK_BYTES]u8 = undefined;
    // SAFETY: `readerDecompressing` initializes this union before it can be
    // read — it is an out-parameter the reader writes through, and the only
    // access to it is via the `reader` returned below.
    var decompress: std.http.Decompress = undefined;
    const reader = response.readerDecompressing(
        &transfer_buffer,
        &decompress,
        decompress_buffer,
    );
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
