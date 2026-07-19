//! Shared QStash ingress wire names and canonical destination construction.

const std = @import("std");

pub const ingress_path = "/v1/ingress/qstash/schedules";
// httpz normalizes request header lookup names to lowercase.
pub const signature_header = "upstash-signature";
pub const schedule_id_header = "upstash-schedule-id";
pub const message_id_header = "upstash-message-id";
pub const max_destination_url_bytes: usize = 2048;

pub fn destinationUrl(buffer: *[max_destination_url_bytes]u8, api_url: []const u8) ![]const u8 {
    const base = std.mem.trimEnd(u8, api_url, "/");
    if (base.len == 0) return error.InvalidApiUrl;
    // The destination rides raw inside the provider request path (QStash parses
    // it itself), so a '?' or '#' here would be read as the QStash request's own
    // query or fragment and silently truncate the callback we register. Reject
    // at construction rather than register a broken schedule.
    if (std.mem.indexOfAny(u8, base, "?#") != null) return error.InvalidApiUrl;
    return std.fmt.bufPrint(buffer, "{s}{s}", .{ base, ingress_path });
}

test "destinationUrl appends the ingress path and trims a trailing slash" {
    var buffer: [max_destination_url_bytes]u8 = undefined;
    try std.testing.expectEqualStrings(
        "https://api.agentsfleet.net" ++ ingress_path,
        try destinationUrl(&buffer, "https://api.agentsfleet.net/"),
    );
}

test "destinationUrl rejects an api url carrying a query or fragment" {
    // Embedded raw in the provider path, these would become the QStash request's
    // own query/fragment and silently truncate the registered callback.
    var buffer: [max_destination_url_bytes]u8 = undefined;
    try std.testing.expectError(error.InvalidApiUrl, destinationUrl(&buffer, "https://api.agentsfleet.net?a=1"));
    try std.testing.expectError(error.InvalidApiUrl, destinationUrl(&buffer, "https://api.agentsfleet.net#frag"));
    try std.testing.expectError(error.InvalidApiUrl, destinationUrl(&buffer, ""));
}
