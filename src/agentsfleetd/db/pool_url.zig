//! Postgres connection-URL parsing — split from `pool.zig` (RULE FLL).
//!
//! URL format: postgres://user:pass@host:port/dbname[?query]. TLS is always
//! required — all role-separated connections go to hosted Postgres providers
//! (PlanetScale, Neon, Supabase) that mandate TLS — except an explicit
//! `?sslmode=disable` for local dev/test docker.

const std = @import("std");
const pg = @import("pg");

const S_SSLMODE = "sslmode=";

// The ONE home for pool sizing. `pool.zig` imports these rather than
// restating them: the pair was duplicated in both files, each comment claiming
// to mirror the other, which is a drift waiting to happen on a value that
// decides how many connections a deployment opens.
//
// Default pool size is a small fraction of the API in-flight-request ceiling:
// many concurrent requests share a handful of DB connections, so the pool need
// not scale 1:1 with request concurrency. Mirrors the
// `API_MAX_IN_FLIGHT_REQUESTS` loader default divided by the per-connection
// request-sharing factor. `initFromEnvForRole` overwrites both from
// env-resolved sizing, so the deployed value can be far larger than this.
const API_MAX_IN_FLIGHT_REQUESTS_DEFAULT: u16 = 256;
const POOL_SIZE_INFLIGHT_DIVISOR: u16 = 64;
pub const POOL_SIZE_DEFAULT: u16 = API_MAX_IN_FLIGHT_REQUESTS_DEFAULT / POOL_SIZE_INFLIGHT_DIVISOR;

// Acquire timeout fails fast: a starved pool surfaces as a quick error rather
// than a multi-second stall that masquerades as a slow request.
pub const ACQUIRE_TIMEOUT_MS_DEFAULT: u32 = 2_000;
const CONNECT_TIMEOUT_MS_DEFAULT: u32 = 10_000;

/// Parse a Postgres connection URL into pg.Pool.Opts.
/// URL format: postgres://user:pass@host:port/dbname[?query]
/// TLS is always required — all role-separated connections go to hosted Postgres
/// providers (PlanetScale, Neon, Supabase) that mandate TLS.
pub fn parseUrl(alloc: std.mem.Allocator, url: []const u8) !pg.Pool.Opts {
    const rest = if (std.mem.startsWith(u8, url, "postgres://"))
        url["postgres://".len..]
    else if (std.mem.startsWith(u8, url, "postgresql://"))
        url["postgresql://".len..]
    else
        return error.InvalidDatabaseUrl;

    const at_pos = std.mem.lastIndexOfScalar(u8, rest, '@') orelse return error.InvalidDatabaseUrl;
    const userpass = rest[0..at_pos];
    const hostpath = rest[at_pos + 1 ..];

    var username: []const u8 = "";
    var password: []const u8 = "";
    if (std.mem.indexOfScalar(u8, userpass, ':')) |colon| {
        username = userpass[0..colon];
        password = userpass[colon + 1 ..];
    } else {
        username = userpass;
    }

    const slash_pos = std.mem.indexOfScalar(u8, hostpath, '/') orelse return error.InvalidDatabaseUrl;
    const hostport = hostpath[0..slash_pos];
    const dbpath = hostpath[slash_pos + 1 ..];

    // Split dbname from query string (e.g. "mydb?sslmode=require" → "mydb", "sslmode=require")
    const query_start = std.mem.indexOfScalar(u8, dbpath, '?');
    const dbname = if (query_start) |q| dbpath[0..q] else dbpath;
    const query_string = if (query_start) |q| dbpath[q + 1 ..] else "";

    // TLS defaults to require (hosted Postgres providers mandate it).
    // Respect ?sslmode=disable for local dev/test with docker Postgres.
    const tls: pg.Conn.Opts.TLS = if (hasSslModeDisable(query_string)) .off else .require;

    var host: []const u8 = hostport;
    var port: u16 = 5432;
    if (std.mem.lastIndexOfScalar(u8, hostport, ':')) |colon| {
        host = hostport[0..colon];
        port = std.fmt.parseInt(u16, hostport[colon + 1 ..], 10) catch return error.InvalidDatabaseUrl;
    }

    // `.size` / `.timeout` (pool acquire timeout) default here and are
    // overwritten from env-resolved sizing (resolveSizing) in initFromEnvForRole.
    // One errdefer per dupe: a failed later dupe frees every earlier one
    // instead of leaking it inside a half-built struct literal.
    const host_owned = try alloc.dupe(u8, host);
    errdefer alloc.free(host_owned);
    const username_owned = try alloc.dupe(u8, username);
    errdefer alloc.free(username_owned);
    const password_owned = try alloc.dupe(u8, password);
    errdefer alloc.free(password_owned);
    const database_owned = try alloc.dupe(u8, dbname);

    return pg.Pool.Opts{
        .size = POOL_SIZE_DEFAULT,
        .timeout = ACQUIRE_TIMEOUT_MS_DEFAULT,
        .connect = .{
            .host = host_owned,
            .port = port,
            .tls = tls,
        },
        .auth = .{
            .username = username_owned,
            .password = password_owned,
            .database = database_owned,
            .timeout = CONNECT_TIMEOUT_MS_DEFAULT,
        },
    };
}

fn hasSslModeDisable(query: []const u8) bool {
    var it = std.mem.splitScalar(u8, query, '&');
    while (it.next()) |param| {
        if (std.mem.startsWith(u8, param, S_SSLMODE)) {
            const val = param[S_SSLMODE.len..];
            if (std.mem.eql(u8, val, "disable")) return true;
        }
    }
    return false;
}

test "hasSslModeDisable detects disable in query string" {
    try std.testing.expect(hasSslModeDisable("sslmode=disable"));
    try std.testing.expect(hasSslModeDisable("application_name=test&sslmode=disable"));
    try std.testing.expect(hasSslModeDisable("sslmode=disable&timeout=10"));
    try std.testing.expect(!hasSslModeDisable("sslmode=require"));
    try std.testing.expect(!hasSslModeDisable("sslmode=verify-full"));
    try std.testing.expect(!hasSslModeDisable(""));
    try std.testing.expect(!hasSslModeDisable("application_name=test"));
}

test "parseUrl dupes the connect strings and frees clean under testing.allocator" {
    // parseUrl is the injectable allocator seam for the pool's connect strings.
    // initFromEnvForRole passes page_allocator there on purpose: pg.Pool.init
    // borrows the strings for the pool's whole life and never copies them, so
    // they are process-lifetime and freed only at exit. Driving the same dupe
    // path on testing.allocator (and freeing it here) proves the allocation side
    // is leak-clean — the audit the production page_allocator site cannot do.
    const a = std.testing.allocator;
    const opts = try parseUrl(a, "postgres://alice:secret@db.example.com:6543/appdb?sslmode=disable");
    defer {
        a.free(opts.connect.host.?);
        a.free(opts.auth.username);
        a.free(opts.auth.password.?);
        a.free(opts.auth.database.?);
    }
    try std.testing.expectEqualStrings("db.example.com", opts.connect.host.?);
    try std.testing.expectEqual(@as(?u16, 6543), opts.connect.port);
    try std.testing.expectEqualStrings("alice", opts.auth.username);
    try std.testing.expectEqualStrings("secret", opts.auth.password.?);
    try std.testing.expectEqualStrings("appdb", opts.auth.database.?);
    try std.testing.expect(opts.connect.tls == .off); // sslmode=disable
}

test "parseUrl survives allocation failure without leaking (errdefer ladder)" {
    // checkAllAllocationFailures fails each of the four dupes in turn and
    // asserts the error return leaks nothing — the proof that a failed later
    // dupe frees every earlier one instead of leaking it in the return literal.
    const Probe = struct {
        fn run(alloc: std.mem.Allocator) !void {
            const opts = try parseUrl(alloc, "postgres://alice:secret@db.example.com:6543/appdb?sslmode=disable");
            alloc.free(opts.connect.host.?);
            alloc.free(opts.auth.username);
            alloc.free(opts.auth.password.?);
            alloc.free(opts.auth.database.?);
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Probe.run, .{});
}
