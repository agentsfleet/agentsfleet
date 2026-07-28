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

// `SupportSummary`, `ManifestEntry`, and `decodeSummaries` are gone. They read
// the persisted support-file manifest back out of `support_files_json` so the
// admin catalog could project it — and nothing on any plane ever rendered what
// they produced. The manifest is still WRITTEN on every import as durable
// provenance for what a stored bundle contained; it is simply no longer read
// back. Support-file BYTES are untouched by all of this: the importer still
// validates and hashes them, and the runner still materializes them from the
// canonical tar, whose entries are the authoritative file list.

pub fn decodeStrings(alloc: std.mem.Allocator, json_text: []const u8) ![]const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, json_text, .{});
}

/// Decode the `{credential_name: reason}` object as a JSON value so it round-trips
/// into the response as a nested object. Rows with no operator-written copy pass
/// the empty-object literal.
pub fn decodeReasons(alloc: std.mem.Allocator, json_text: []const u8) !std.json.Value {
    return std.json.parseFromSliceLeaky(std.json.Value, alloc, json_text, .{});
}
