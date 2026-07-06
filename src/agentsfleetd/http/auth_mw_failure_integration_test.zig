// Proves the auth-middleware-chain generic-error catch branch (server.zig's
// `auth_mw.run(...) catch |e|`) no longer leaks a raw Zig error-union tag
// into the caller-visible response.
//
// Wires a fake Middleware(AuthCtx) onto the svix webhook slot whose
// execute_fn unconditionally returns a simulated infra fault — this is the
// same shape the production middlewares use (chain.Middleware is a plain
// function-pointer/erased struct), so no server.zig change is required to
// drive the real dispatchMatchedRoute catch branch deterministically.

const std = @import("std");
const httpz = @import("httpz");
const auth_mw = @import("../auth/middleware/mod.zig");
const chain = @import("../auth/middleware/chain.zig");
const harness_mod = @import("test_harness.zig");
const ec = @import("../errors/error_registry.zig");
const server = @import("server.zig");

const TestHarness = harness_mod.TestHarness;
const AuthCtx = auth_mw.AuthCtx;

const FaultyMiddleware = struct {
    fn executeTypeErased(_: *anyopaque, _: *AuthCtx, _: *httpz.Request) anyerror!chain.Outcome {
        return error.SimulatedAuthInfraFault;
    }

    fn middleware() chain.Middleware(AuthCtx) {
        // SAFETY: `dummy` is never read — executeTypeErased ignores `ptr` entirely
        // (this fake middleware always errors before touching any state).
        var dummy: u8 = undefined;
        return .{ .ptr = @ptrCast(&dummy), .execute_fn = executeTypeErased };
    }
};

fn wireFaultyMiddleware(reg: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {
    reg.setSvixSig(FaultyMiddleware.middleware());
}

test "auth-mw chain failure never leaks a raw @errorName tag in the response" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = wireFaultyMiddleware });
    defer h.deinit();

    const req = try h.post("/v1/webhooks/svix/z1").json("{}");
    const r = try req.send();
    defer r.deinit();

    try r.expectStatus(.internal_server_error);
    try r.expectErrorCode(ec.ERR_INTERNAL_OPERATION_FAILED);
    // No raw Zig error tag reaches the wire — neither the simulated fault's
    // own name nor the generic `error.` union-tag spelling.
    try std.testing.expect(std.mem.indexOf(u8, r.body, "SimulatedAuthInfraFault") == null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "error.") == null);
}

test "auth-mw chain failure response is deterministic (stable code + message)" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = wireFaultyMiddleware });
    defer h.deinit();

    const req1 = try h.post("/v1/webhooks/svix/z1").json("{}");
    const r1 = try req1.send();
    defer r1.deinit();
    const req2 = try h.post("/v1/webhooks/svix/z2").json("{}");
    const r2 = try req2.send();
    defer r2.deinit();

    try r1.expectStatus(.internal_server_error);
    try r2.expectStatus(.internal_server_error);
    try r1.expectErrorCode(ec.ERR_INTERNAL_OPERATION_FAILED);
    try r2.expectErrorCode(ec.ERR_INTERNAL_OPERATION_FAILED);
    var needle_buf: [256]u8 = undefined;
    const needle = try std.fmt.bufPrint(&needle_buf, "\"detail\":\"{s}\"", .{server.S_AUTH_MW_FAILURE_DETAIL});
    try std.testing.expect(std.mem.indexOf(u8, r1.body, needle) != null);
    try std.testing.expect(std.mem.indexOf(u8, r2.body, needle) != null);
}
