//! Reads a user's provisioned capability claim from the identity provider's
//! backend Application Programming Interface (API): `GET /users/{subject}`,
//! returning `public_metadata.scopes` verbatim — the same space-delimited
//! string the session-token template projects onto a JavaScript Object
//! Notation (JSON) Web Token, so both credential shapes feed one `parseClaim`.
//!
//! Why a second transport rather than reuse. `jwks_fetch.fetchCapped` performs
//! an unauthenticated GET and takes no headers; this call must carry the
//! backend secret. `http/handlers/connectors/bounded_fetch.zig` does take
//! headers, but it lives in the business layer and `src/auth/` may not import
//! it — `make test-auth` compiles this tree in isolation and would fail.
//! Widening either was the alternative; both belong to callers with different
//! threat models (a config-supplied key-set Uniform Resource Locator, a vendor
//! endpoint under a deadline scheduler), so this path carries its own bound.
//!
//! Fail-closed, not fail-loud, on a provisioning gap: a subject with no
//! `scopes` key — or one holding a non-string — answers an empty claim, which
//! `parseClaim` turns into an empty capability set and every gate then refuses.
//! Only a provider that is unreachable or answers unparseable bytes is an
//! error, because that is the one case where the caller's capabilities are
//! genuinely unknown rather than genuinely absent.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const ec = @import("auth_codes");
const clerk_backend = @import("clerk_backend.zig");
const jwks_standard_claims = @import("jwks_standard_claims.zig");

const log = logging.scoped(.clerk_scopes);

/// Upper bound on the user document, counted in bytes handed back by the
/// client after any content-encoding is decoded. A provisioned user object is
/// a few kilobytes; this leaves two orders of magnitude of headroom and still
/// refuses a response that could only be a fault or a decompression bomb.
pub const USER_MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Only one hop is expected — the backend API answers directly. Following a
/// redirect would mean sending the backend secret to a host the configuration
/// never named, so redirects are refused rather than chased.
const REDIRECT_BEHAVIOR: std.http.Client.Request.RedirectBehavior = .unhandled;

const AUTHORIZATION_HEADER = "authorization";
const ACCEPT_HEADER = "accept";
const ACCEPT_JSON = "application/json";
const PUBLIC_METADATA_KEY = "public_metadata";
const SCOPES_KEY = "scopes";

/// Claim answered when the provider holds no scope provisioning for a subject.
/// Named because the empty string is a decision here (fail closed), not an
/// accident of a missing branch.
pub const UNPROVISIONED_CLAIM = "";

const EV_UNPROVISIONED = "subject_has_no_scope_claim";
const EV_UNEXPECTED_STATUS = "unexpected_status";

pub const FetchError = error{
    OutOfMemory,
    MissingSecret,
    FetchFailed,
    ResponseTooLarge,
    Unauthorized,
    NotFound,
};

/// Fetch the space-delimited capability claim provisioned for `oidc_subject`.
/// The caller owns the returned bytes and frees them with `alloc`.
///
/// Returns `UNPROVISIONED_CLAIM` (duplicated, so ownership is uniform) when the
/// user exists but carries no scope provisioning. `NotFound` is reserved for a
/// subject the provider does not know at all, which means the local row and the
/// provider have diverged and is worth telling apart from "nothing granted".
pub fn fetchScopeClaim(
    alloc: std.mem.Allocator,
    secret: ?[]const u8,
    oidc_subject: []const u8,
) FetchError![]const u8 {
    // Borrowed from the boot-resolved secret, exactly as the metadata writer
    // borrows it — no per-request environment read and nothing to free here.
    const backend_secret = secret orelse return FetchError.MissingSecret;
    if (std.mem.trim(u8, backend_secret, " \t\r\n").len == 0) return FetchError.MissingSecret;

    const url = try std.fmt.allocPrint(alloc, "{s}/users/{s}", .{ clerk_backend.API_BASE, oidc_subject });
    defer alloc.free(url);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{backend_secret});
    defer alloc.free(auth_header);

    const body = try getCapped(alloc, url, auth_header);
    defer alloc.free(body);

    return extractScopeClaim(alloc, body);
}

/// One bounded, authenticated GET. The response lands in a single fixed-size
/// buffer: a body past the cap overflows the writer, which surfaces as
/// `ResponseTooLarge` rather than growing an accumulator the caller never
/// bounded. Caller owns the returned bytes.
fn getCapped(
    alloc: std.mem.Allocator,
    url: []const u8,
    auth_header: []const u8,
) FetchError![]u8 {
    // Outbound read on a request thread — a blocking one-shot, so the
    // process-global blocking io is the correct source, matching the sibling
    // metadata writer.
    var client: std.http.Client = .{ .allocator = alloc, .io = common.globalIo() };
    defer client.deinit();

    const headers: [2]std.http.Header = .{
        .{ .name = AUTHORIZATION_HEADER, .value = auth_header },
        .{ .name = ACCEPT_HEADER, .value = ACCEPT_JSON },
    };

    // BUFFER GATE: fixed writer over one heap buffer — the cap IS the bound,
    // so a writer that cannot grow past it is the enforcement, not a check
    // after the fact.
    const buffer = alloc.alloc(u8, USER_MAX_RESPONSE_BYTES) catch return FetchError.OutOfMemory;
    defer alloc.free(buffer);
    var response_writer = std.Io.Writer.fixed(buffer);

    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
        .extra_headers = &headers,
        .redirect_behavior = REDIRECT_BEHAVIOR,
        .response_writer = &response_writer,
    }) catch |err| return mapFetchError(err);

    try mapStatus(@intFromEnum(result.status), oidcSubjectFreeUrl(url));
    return alloc.dupe(u8, response_writer.buffered()) catch FetchError.OutOfMemory;
}

/// The subject rides the path, and a log line naming a specific person's
/// identifier on every provider hiccup is attribution nobody asked for. Trim
/// back to the collection so the operator still sees which endpoint answered.
fn oidcSubjectFreeUrl(url: []const u8) []const u8 {
    const last_slash = std.mem.lastIndexOfScalar(u8, url, '/') orelse return url;
    return url[0..last_slash];
}

/// Pure status mapping, extracted so every branch is provable without standing
/// up a listener. A 404 is deliberately distinct from an empty claim: it says
/// the local row outlived the provider's user.
pub fn mapStatus(status: u16, url: []const u8) FetchError!void {
    if (status >= 200 and status < 300) return;
    if (status == 401 or status == 403) return FetchError.Unauthorized;
    if (status == 404) return FetchError.NotFound;
    log.warn(EV_UNEXPECTED_STATUS, .{
        .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
        .status = status,
        .url = url,
    });
    return FetchError.FetchFailed;
}

/// Pull `public_metadata.scopes` out of a user document. Every shape that is
/// not a present string answers the unprovisioned claim, so a hand-edited
/// metadata object narrows a principal instead of failing a request open.
/// Caller owns the returned bytes.
pub fn extractScopeClaim(alloc: std.mem.Allocator, body: []const u8) FetchError![]const u8 {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch
        return FetchError.FetchFailed;
    defer parsed.deinit();

    const root = switch (parsed.value) {
        .object => |o| o,
        else => return FetchError.FetchFailed,
    };
    const metadata = switch (root.get(PUBLIC_METADATA_KEY) orelse return unprovisioned(alloc)) {
        .object => |o| o,
        else => return unprovisioned(alloc),
    };
    const claim = jwks_standard_claims.getString(metadata, SCOPES_KEY) orelse
        return unprovisioned(alloc);
    return alloc.dupe(u8, claim) catch FetchError.OutOfMemory;
}

/// Owned copy of the empty claim, so callers free the result unconditionally
/// and no branch returns a borrow of a literal.
fn unprovisioned(alloc: std.mem.Allocator) FetchError![]const u8 {
    log.debug(EV_UNPROVISIONED, .{});
    return alloc.dupe(u8, UNPROVISIONED_CLAIM) catch FetchError.OutOfMemory;
}

fn mapFetchError(err: anyerror) FetchError {
    return switch (err) {
        error.WriteFailed => FetchError.ResponseTooLarge,
        error.OutOfMemory => FetchError.OutOfMemory,
        else => FetchError.FetchFailed,
    };
}

// In-file tests: `getCapped` and `mapFetchError` are deliberately private, and
// the properties under proof — that a real response round-trips, and that a
// failure retains nothing — are allocator-visible only from inside this module.
// The pure status and claim surfaces are tested through the sibling file.

fn testBoundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

const TestUserServer = struct {
    fn run(listener: *std.Io.net.Server, io: std.Io, response: []const u8) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [2048]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        var sent: usize = 0;
        while (sent < response.len) {
            const rc = std.posix.system.write(conn.socket.handle, response[sent..].ptr, response.len - sent);
            if (std.posix.errno(rc) != .SUCCESS) return;
            sent += @intCast(rc);
        }
    }
};

test "a real provider response round-trips into a capability claim" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();
    var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", 0) catch return error.SkipZigTest;
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = testBoundPort(listener.socket.handle) catch return error.SkipZigTest;

    const body =
        \\{"id":"user_2aXyTest","public_metadata":{"scopes":"fleet:read model:read"}}
    ;
    const response = try std.fmt.allocPrint(
        alloc,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {d}\r\nconnection: close\r\n\r\n{s}",
        .{ body.len, body },
    );
    defer alloc.free(response);
    const server = std.Thread.spawn(.{}, TestUserServer.run, .{ &listener, io, response }) catch
        return error.SkipZigTest;

    var url_buf: [64]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}/users/user_2aXyTest", .{port});
    const raw = getCapped(alloc, url, "Bearer sk_test");
    server.join();

    const received = try raw;
    defer alloc.free(received);
    const claim = try extractScopeClaim(alloc, received);
    defer alloc.free(claim);
    try std.testing.expectEqualStrings("fleet:read model:read", claim);
}

test "an unreachable provider retains nothing under the leak-detecting allocator" {
    const r = getCapped(std.testing.allocator, "http://127.0.0.1:9/users/u", "Bearer sk_test");
    if (r) |body| {
        std.testing.allocator.free(body);
        return error.TestUnexpectedResult;
    } else |_| {}
}

test "a body past the cap is refused rather than truncated into a claim" {
    // The fixed writer cannot grow, so an oversized body surfaces as a failed
    // write. Mapping it to its own error keeps "implausible response" distinct
    // from "provider unreachable" for whoever reads the log.
    try std.testing.expectEqual(FetchError.ResponseTooLarge, mapFetchError(error.WriteFailed));
    try std.testing.expectEqual(FetchError.OutOfMemory, mapFetchError(error.OutOfMemory));
    try std.testing.expectEqual(FetchError.FetchFailed, mapFetchError(error.ConnectionRefused));
}

test {
    // Keeps every declaration analysed even where nothing in this tree calls
    // it yet — an unreferenced body is never type-checked, which is how a
    // sibling module reached review with a green build and two errors in it.
    std.testing.refAllDecls(@This());
    _ = @import("clerk_scope_fetch_test.zig");
}
