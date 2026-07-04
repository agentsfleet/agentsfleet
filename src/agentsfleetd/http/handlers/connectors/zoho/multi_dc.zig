//! Zoho multi-data-center resolution (docs: zoho.com/accounts/protocol/oauth/multi-dc.html).
//! Zoho's authorize step always starts at the US accounts server regardless of
//! the user's region; on callback it appends `location` (one of
//! us/eu/in/au/cn/jp/ca) naming the data center that actually issued the
//! `code`. The code is only redeemable at THAT data center's accounts
//! server — exchanging (or later refreshing) at the wrong one fails
//! `invalid_grant`. Every DC's accounts server is `accounts.zoho.<tld>`
//! except Canada, whose accounts server lives on a different apex domain
//! entirely (`accounts.zohocloud.ca`) — the one irregular entry in Zoho's
//! own DC table, not a suffix Zoho ever put under `zoho.ca`.

const std = @import("std");

const DataCenter = enum { us, eu, in_, au, cn, jp, ca };

fn dataCenter(location: ?[]const u8) DataCenter {
    const loc = location orelse return .us;
    inline for (.{
        .{ "eu", .eu },
        .{ "in", .in_ },
        .{ "au", .au },
        .{ "cn", .cn },
        .{ "jp", .jp },
        .{ "ca", .ca },
    }) |entry| {
        if (std.mem.eql(u8, loc, entry[0])) return entry[1];
    }
    // "us" or anything unrecognized — fail-safe default, matches the
    // pre-fix behavior for the common case.
    return .us;
}

/// The accounts server for this data center — the value to persist on the
/// vault handle for future refresh mints.
pub fn accountsBase(location: ?[]const u8) []const u8 {
    return switch (dataCenter(location)) {
        .us => "https://accounts.zoho.com",
        .eu => "https://accounts.zoho.eu",
        .in_ => "https://accounts.zoho.in",
        .au => "https://accounts.zoho.com.au",
        .cn => "https://accounts.zoho.com.cn",
        .jp => "https://accounts.zoho.jp",
        .ca => "https://accounts.zohocloud.ca",
    };
}

/// The token endpoint for this data center — used to override the initial
/// code exchange, which must hit the same accounts server that issued the
/// code.
pub fn tokenEndpoint(location: ?[]const u8) []const u8 {
    return switch (dataCenter(location)) {
        .us => "https://accounts.zoho.com/oauth/v2/token",
        .eu => "https://accounts.zoho.eu/oauth/v2/token",
        .in_ => "https://accounts.zoho.in/oauth/v2/token",
        .au => "https://accounts.zoho.com.au/oauth/v2/token",
        .cn => "https://accounts.zoho.com.cn/oauth/v2/token",
        .jp => "https://accounts.zoho.jp/oauth/v2/token",
        .ca => "https://accounts.zohocloud.ca/oauth/v2/token",
    };
}

test "accountsBase: known data centers map to their accounts server" {
    try std.testing.expectEqualStrings("https://accounts.zoho.com", accountsBase(null));
    try std.testing.expectEqualStrings("https://accounts.zoho.com", accountsBase("us"));
    try std.testing.expectEqualStrings("https://accounts.zoho.eu", accountsBase("eu"));
    try std.testing.expectEqualStrings("https://accounts.zoho.in", accountsBase("in"));
    try std.testing.expectEqualStrings("https://accounts.zoho.com.au", accountsBase("au"));
    try std.testing.expectEqualStrings("https://accounts.zoho.com.cn", accountsBase("cn"));
    try std.testing.expectEqualStrings("https://accounts.zoho.jp", accountsBase("jp"));
}

test "accountsBase: Canada is the irregular DC — zohocloud.ca, not zoho.ca" {
    try std.testing.expectEqualStrings("https://accounts.zohocloud.ca", accountsBase("ca"));
}

test "accountsBase: unrecognized location falls back to US (fail-safe default)" {
    try std.testing.expectEqualStrings("https://accounts.zoho.com", accountsBase("mars"));
    try std.testing.expectEqualStrings("https://accounts.zoho.com", accountsBase(""));
}

test "tokenEndpoint: mirrors accountsBase with the token path appended" {
    try std.testing.expectEqualStrings("https://accounts.zoho.com/oauth/v2/token", tokenEndpoint(null));
    try std.testing.expectEqualStrings("https://accounts.zoho.eu/oauth/v2/token", tokenEndpoint("eu"));
    try std.testing.expectEqualStrings("https://accounts.zohocloud.ca/oauth/v2/token", tokenEndpoint("ca"));
}
