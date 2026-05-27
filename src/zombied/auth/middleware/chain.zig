//! Runtime middleware chain for the HTTP auth pipeline (M18_002).
//!
//! Modeled on httpz's `Middleware(H)` — generic over the request-scoped
//! context so `src/auth/` stays free of handler/HTTP-layer imports and
//! remains extractable into a standalone `zombie-auth` repository.
//!
//! A chain runs middlewares in order. The first one to write a response
//! returns `.short_circuit`; subsequent middlewares and the handler are
//! skipped. All `.next` outcomes fall through to the handler.

const std = @import("std");
const httpz = @import("httpz");

pub const Outcome = enum { next, short_circuit };

/// Type-erased middleware entry generic over a request-scoped context.
///
/// Callers instantiate with their own context type (e.g. `Hx`) — the
/// auth layer itself never needs to know its concrete shape.
pub fn Middleware(comptime Ctx: type) type {
    return struct {
        const Self = @This();

        ptr: *anyopaque,
        execute_fn: *const fn (ptr: *anyopaque, ctx: *Ctx, req: *httpz.Request) anyerror!Outcome,

        pub fn execute(self: Self, ctx: *Ctx, req: *httpz.Request) !Outcome {
            return self.execute_fn(self.ptr, ctx, req);
        }
    };
}

/// Run the chain in order. Returns `.short_circuit` as soon as any
/// middleware signals it wrote a response; otherwise returns `.next`.
pub fn run(
    comptime Ctx: type,
    chain: []const Middleware(Ctx),
    ctx: *Ctx,
    req: *httpz.Request,
) !Outcome {
    for (chain) |m| {
        switch (try m.execute(ctx, req)) {
            .next => continue,
            .short_circuit => return .short_circuit,
        }
    }
    return .next;
}

// ── Tests ────────────────────────────────────────────────────────────────
//
// The chain runner is Ctx-generic. Tests below use a tiny `TestCtx` that
// records which middlewares ran, so we can assert short-circuit semantics
// without depending on the real `Hx` type (which lives outside src/auth/).

const testing = std.testing;

const TestCtx = struct {
    /// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
    calls: std.ArrayList([]const u8),
};

fn makeRecorder(comptime name: []const u8, comptime outcome: Outcome) fn (*anyopaque, *TestCtx, *httpz.Request) anyerror!Outcome {
    return struct {
        fn execute(_: *anyopaque, ctx: *TestCtx, _: *httpz.Request) anyerror!Outcome {
            try ctx.calls.append(testing.allocator, name);
            return outcome;
        }
    }.execute;
}

test "run() invokes every middleware in order when all return .next" {
    var dummy_anyopaque: u8 = 0;
    var ctx = TestCtx{ .calls = .{} };
    defer ctx.calls.deinit(testing.allocator);

    const chain: []const Middleware(TestCtx) = &.{
        .{ .ptr = &dummy_anyopaque, .execute_fn = makeRecorder("a", .next) },
        .{ .ptr = &dummy_anyopaque, .execute_fn = makeRecorder("b", .next) },
        .{ .ptr = &dummy_anyopaque, .execute_fn = makeRecorder("c", .next) },
    };

    var req: httpz.Request = undefined;
    const outcome = try run(TestCtx, chain, &ctx, &req);

    try testing.expectEqual(Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 3), ctx.calls.items.len);
    try testing.expectEqualStrings("a", ctx.calls.items[0]);
    try testing.expectEqualStrings("b", ctx.calls.items[1]);
    try testing.expectEqualStrings("c", ctx.calls.items[2]);
}

test "run() short-circuits and skips remaining middlewares" {
    var dummy_anyopaque: u8 = 0;
    var ctx = TestCtx{ .calls = .{} };
    defer ctx.calls.deinit(testing.allocator);

    const chain: []const Middleware(TestCtx) = &.{
        .{ .ptr = &dummy_anyopaque, .execute_fn = makeRecorder("first", .next) },
        .{ .ptr = &dummy_anyopaque, .execute_fn = makeRecorder("gate", .short_circuit) },
        .{ .ptr = &dummy_anyopaque, .execute_fn = makeRecorder("sentinel", .next) },
    };

    var req: httpz.Request = undefined;
    const outcome = try run(TestCtx, chain, &ctx, &req);

    try testing.expectEqual(Outcome.short_circuit, outcome);
    try testing.expectEqual(@as(usize, 2), ctx.calls.items.len);
    try testing.expectEqualStrings("first", ctx.calls.items[0]);
    try testing.expectEqualStrings("gate", ctx.calls.items[1]);
}
