//! Shared projection for every Fleet library read: the workspace gallery
//! (`gallery.zig`) and the operator catalog (`catalog.zig`).
//!
//! Both surfaces answer from the same columns, so the shapes and the decoders
//! live here once. The load-bearing property is what these types CANNOT express:
//! there is no field for `skill_markdown`, `trigger_markdown`, a support-file
//! body, or an object-store key. A read cannot leak bundle content because the
//! struct it would have to leak through does not exist (M128 Invariant 3).

const std = @import("std");

/// Requirements as the API reports them. `trigger_present` is a flag, never the
/// TRIGGER.md body.
pub const Requirements = struct {
    credentials: []const []const u8,
    tools: []const []const u8,
    network_hosts: []const []const u8,
    trigger_present: bool,
};

/// A support file as the API reports it: path and size. The per-file hash stays
/// internal — it is a handle to stored bytes, and no reader needs it.
pub const SupportSummary = struct {
    path: []const u8,
    size_bytes: usize,
};

/// The manifest entry shape persisted in `support_files_json`. Decoded only to
/// project `SupportSummary` out of it; `sha256` is read and dropped.
pub const ManifestEntry = struct {
    path: []const u8,
    size_bytes: usize,
    sha256: []const u8 = "",
};

pub fn decodeStrings(alloc: std.mem.Allocator, json_text: []const u8) ![]const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, json_text, .{});
}

/// Decode the `{credential_name: reason}` object as a JSON value so it round-trips
/// into the response as a nested object. Rows with no operator-written copy pass
/// the empty-object literal.
pub fn decodeReasons(alloc: std.mem.Allocator, json_text: []const u8) !std.json.Value {
    return std.json.parseFromSliceLeaky(std.json.Value, alloc, json_text, .{});
}

/// Project the stored manifest into {path, size_bytes} summaries. A manifest that
/// fails to parse degrades to zero support files rather than failing the whole
/// read — one malformed row must not take down the operator's catalog view.
pub fn decodeSummaries(alloc: std.mem.Allocator, json_text: []const u8) ![]const SupportSummary {
    const manifest = std.json.parseFromSliceLeaky([]const ManifestEntry, alloc, json_text, .{ .ignore_unknown_fields = true }) catch {
        return &.{};
    };
    const out = try alloc.alloc(SupportSummary, manifest.len);
    for (manifest, 0..) |entry, i| out[i] = .{ .path = entry.path, .size_bytes = entry.size_bytes };
    return out;
}
