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
    return std.fmt.bufPrint(buffer, "{s}{s}", .{ base, ingress_path });
}
