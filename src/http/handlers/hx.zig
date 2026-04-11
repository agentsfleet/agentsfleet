// M11_002: Request-scoped handler context and comptime auth wrappers.
//
// Eliminates the boilerplate ritual (arena, req_id, authenticate,
// auth-error response) from every authenticated handler.
//
// Usage:
//   fn innerCreateZombie(hx: Hx, req: *httpz.Request) void { ... }
//   pub const handleCreateZombie = authenticated(innerCreateZombie);
//
//   fn innerDeleteZombie(hx: Hx, req: *httpz.Request, zombie_id: []const u8) void { ... }
//   pub const handleDeleteZombie = authenticatedWithParam(innerDeleteZombie);

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");

pub const Hx = struct {
    alloc: std.mem.Allocator,
    principal: common.AuthPrincipal,
    req_id: []const u8,
    ctx: *common.Context,
    res: *httpz.Response,

    /// Write a successful JSON response.
    pub fn ok(self: Hx, status: std.http.Status, body: anytype) void {
        common.writeJson(self.res, status, body);
    }

    /// Write an RFC 7807 error response. HTTP status is owned by the error code table.
    pub fn fail(self: Hx, code: []const u8, detail: []const u8) void {
        common.errorResponse(self.res, code, detail, self.req_id);
    }
};

/// Returns an httpz-compatible handler fn that:
///   1. Sets up an arena allocator (freed on return).
///   2. Generates a request ID.
///   3. Calls common.authenticate — returns rich auth error on failure.
///   4. Builds Hx and calls inner(hx, req).
pub fn authenticated(
    comptime inner: fn (hx: Hx, req: *httpz.Request) void,
) fn (*common.Context, *httpz.Request, *httpz.Response) void {
    return struct {
        fn handle(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response) void {
            var arena = std.heap.ArenaAllocator.init(ctx.alloc);
            defer arena.deinit();
            const alloc = arena.allocator();
            const req_id = common.requestId(alloc);

            const principal = common.authenticate(alloc, req, ctx) catch |err| {
                common.writeAuthError(ctx, res, req_id, err);
                return;
            };

            inner(.{
                .alloc = alloc,
                .principal = principal,
                .req_id = req_id,
                .ctx = ctx,
                .res = res,
            }, req);
        }
    }.handle;
}

/// Like authenticated(), but the inner function also receives a path param.
pub fn authenticatedWithParam(
    comptime inner: fn (hx: Hx, req: *httpz.Request, param: []const u8) void,
) fn (*common.Context, *httpz.Request, *httpz.Response, []const u8) void {
    return struct {
        fn handle(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, param: []const u8) void {
            var arena = std.heap.ArenaAllocator.init(ctx.alloc);
            defer arena.deinit();
            const alloc = arena.allocator();
            const req_id = common.requestId(alloc);

            const principal = common.authenticate(alloc, req, ctx) catch |err| {
                common.writeAuthError(ctx, res, req_id, err);
                return;
            };

            inner(.{
                .alloc = alloc,
                .principal = principal,
                .req_id = req_id,
                .ctx = ctx,
                .res = res,
            }, req, param);
        }
    }.handle;
}
