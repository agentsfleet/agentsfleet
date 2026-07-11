//! Tests for child_supervisor's stdout read path and fail-closed sandbox setup.
//! Extracted to a sibling so the supervisor file stays within the line budget;
//! consumes `readResult` + `establishSandbox` as the in-tree test consumer.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const builtin = @import("builtin");
const supervisor = @import("child_supervisor.zig");
const pipe_proto = @import("pipe_proto.zig");
const client_errors = @import("engine/client_errors.zig");
const contract = @import("contract");

const cgroup = @import("engine/CgroupScope.zig");
const cred = @import("engine/credential_request.zig");

const ActivityFrame = contract.activity.ActivityFrame;
const ActivitySink = supervisor.ActivitySink;
const FailureClass = contract.execution_result.FailureClass;

// No-op memory sink: capture-frame forwarding is irrelevant to these read-path
// tests (covered by inrun_memory + loop tests), so drop every `.memory` frame.
const NoopMem = struct {
    var dummy: u8 = 0;
    fn forward(_: *anyopaque, _: []const u8) void {}
    fn sink() supervisor.MemorySink {
        return .{ .ctx = &dummy, .forward = forward };
    }
};

test "readResult forwards activity frames in order and returns the result frame" {
    const Cap = struct {
        count: usize = 0,
        name_buf: [64]u8 = [_]u8{0} ** 64,
        name_len: usize = 0,
        fn forward(ctx: *anyopaque, frame: ActivityFrame) void {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.count += 1;
            if (frame == .tool_call_started) {
                const n = frame.tool_call_started.name;
                @memcpy(self.name_buf[0..n.len], n);
                self.name_len = n.len;
            }
        }
    };

    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    const af = ActivityFrame{ .tool_call_started = .{ .name = "fly_deploy", .args_redacted = "{}" } };
    const af_json = try std.json.Stringify.valueAlloc(std.testing.allocator, af, .{});
    defer std.testing.allocator.free(af_json);
    try pipe_proto.writeFrame(fds[1], .activity, af_json);
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(fds[1]);

    var cap: Cap = .{};
    const sink = ActivitySink{ .ctx = &cap, .forward = Cap.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, sink, NoopMem.sink(), null, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(!outcome.timed_out);
    try std.testing.expect(!outcome.terminated);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);
    try std.testing.expectEqual(@as(usize, 1), cap.count);
    try std.testing.expectEqualStrings("fly_deploy", cap.name_buf[0..cap.name_len]);
}

test "readResult forwards a memory frame's raw bytes to the memory sink, then the result" {
    // A run that writes N memory entries surfaces them as one `.memory` frame;
    // the parent must forward that payload verbatim (the daemon parses + POSTs
    // the N deltas). Proves the capture path the daemon push rides.
    const Cap = struct {
        count: usize = 0,
        buf: [256]u8 = [_]u8{0} ** 256,
        len: usize = 0,
        fn forward(ctx: *anyopaque, payload: []const u8) void {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.count += 1;
            @memcpy(self.buf[0..payload.len], payload);
            self.len = payload.len;
        }
    };
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    const mem_json = "[{\"key\":\"a\",\"content\":\"1\",\"category\":\"core\"},{\"key\":\"b\",\"content\":\"2\",\"category\":\"core\"}]";
    try pipe_proto.writeFrame(fds[1], .memory, mem_json);
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(fds[1]);

    var dummy: u8 = 0;
    const act_sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };
    var cap: Cap = .{};
    const mem_sink = supervisor.MemorySink{ .ctx = &cap, .forward = Cap.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, act_sink, mem_sink, null, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expectEqual(@as(usize, 1), cap.count);
    try std.testing.expectEqualStrings(mem_json, cap.buf[0..cap.len]);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);

    // The two deltas survive a parse — the daemon would POST exactly these.
    const parsed = try std.json.parseFromSlice([]@import("contract").protocol.MemoryDelta, std.testing.allocator, cap.buf[0..cap.len], .{});
    defer parsed.deinit();
    try std.testing.expectEqual(@as(usize, 2), parsed.value.len);
}

test "readResult tolerates a malformed activity frame and still returns the result" {
    const Noop = struct {
        fn forward(_: *anyopaque, _: ActivityFrame) void {}
    };
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    try pipe_proto.writeFrame(fds[1], .activity, "{not valid json"); // dropped
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":false}");
    pipe_proto.testOsClose(fds[1]);

    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = Noop.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, sink, NoopMem.sink(), null, null);
    defer std.testing.allocator.free(outcome.bytes);
    try std.testing.expectEqualStrings("{\"exit_ok\":false}", outcome.bytes);
}

// A scripted renewal hook: returns a queued decision per tick so a test can
// drive readResult's renewal path deterministically without any HTTP.
const ScriptedHook = struct {
    decisions: []const supervisor.RenewDecision,
    idx: usize = 0,
    ticks: usize = 0,
    /// Cumulative snapshot the most recent tick observed — zeros until the
    /// child's first usage frame, per the RenewHook doc.
    last_usage: supervisor.UsageSnapshot = .{},
    fn onTick(ctx: *anyopaque, now_ms: i64, usage: supervisor.UsageSnapshot) supervisor.RenewDecision {
        _ = now_ms;
        const self: *ScriptedHook = @ptrCast(@alignCast(ctx));
        self.ticks += 1;
        self.last_usage = usage;
        if (self.idx >= self.decisions.len) return .keep;
        const d = self.decisions[self.idx];
        self.idx += 1;
        return d;
    }
};

const NoopSink = struct {
    fn forward(_: *anyopaque, _: ActivityFrame) void {}
};

test "readResult: a hook returning .terminate kills the wait and reports terminated" {
    // A pipe with no data and an open write end: the read blocks, so the tick
    // fires. The scripted hook terminates on the first tick. A far deadline
    // proves termination came from the hook, not a timeout.
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    defer pipe_proto.testOsClose(fds[1]); // keep write end open → no EOF, forces a tick

    var hook_state = ScriptedHook{ .decisions = &.{.{ .terminate = .renewal_terminate }} };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };

    const dl = clock.nowMillis() + 60_000; // far; the 10ms tick fires first
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, sink, NoopMem.sink(), hook, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(outcome.terminated);
    try std.testing.expect(!outcome.timed_out);
    // A hook that names no reason keeps the historical attribution.
    try std.testing.expectEqual(contract.execution_result.FailureClass.renewal_terminate, outcome.terminate_reason);
    try std.testing.expect(hook_state.ticks >= 1);
    // No usage frame ever arrived → the tick observed all-zero counters.
    try std.testing.expectEqual(supervisor.UsageSnapshot{}, hook_state.last_usage);
}

test "readResult round-trips one usage frame into the snapshot the tick observes" {
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    const snap = pipe_proto.UsageSnapshot{ .input_tokens = 7, .cached_input_tokens = 1, .output_tokens = 3 };
    const payload = snap.encode();
    try pipe_proto.writeFrame(fds[1], .usage, &payload);
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(fds[1]);

    var hook_state = ScriptedHook{ .decisions = &.{} }; // every tick keeps
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10_000 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, sink, NoopMem.sink(), hook, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);
    try std.testing.expect(hook_state.ticks >= 1); // the usage frame is a renewal point
    try std.testing.expectEqual(snap, hook_state.last_usage);
}

test "readResult folds a regressed usage frame with max so the snapshot never walks backwards" {
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    const first = pipe_proto.UsageSnapshot{ .input_tokens = 100, .cached_input_tokens = 0, .output_tokens = 40 };
    const first_payload = first.encode();
    try pipe_proto.writeFrame(fds[1], .usage, &first_payload);
    // A restarted child re-sends lower cumulatives — they must not regress the fold.
    const regressed = pipe_proto.UsageSnapshot{ .input_tokens = 50, .cached_input_tokens = 0, .output_tokens = 20 };
    const regressed_payload = regressed.encode();
    try pipe_proto.writeFrame(fds[1], .usage, &regressed_payload);
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(fds[1]);

    var hook_state = ScriptedHook{ .decisions = &.{} };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10_000 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, sink, NoopMem.sink(), hook, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(hook_state.ticks >= 2); // one renewal point per usage frame
    try std.testing.expectEqual(first, hook_state.last_usage); // max-fold won, not the regressed frame
}

test "a malformed usage frame is dropped and the last-known counters survive" {
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    const good = pipe_proto.UsageSnapshot{ .input_tokens = 100, .output_tokens = 40 };
    const good_payload = good.encode();
    try pipe_proto.writeFrame(fds[1], .usage, &good_payload);
    try pipe_proto.writeFrame(fds[1], .usage, good_payload[0 .. pipe_proto.UsageSnapshot.WIRE_LEN - 1]); // truncated: one byte short
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(fds[1]);

    var hook_state = ScriptedHook{ .decisions = &.{} };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10_000 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], dl, sink, NoopMem.sink(), hook, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes); // run unaffected
    try std.testing.expectEqual(good, hook_state.last_usage); // kept, never zeroed or invented
}

test "readResult: a hook .extend past a near deadline keeps reading to the result" {
    // The lease deadline is near (500ms); without renewal the wait would time
    // out before the writer (1000ms) sends the result. The hook extends on an
    // early 10ms tick, so the result is still read cleanly. Margins are sized
    // at 50× the tick so scheduling jitter on a loaded CI box can't fire the
    // deadline before the extend lands.
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);

    var hook_state = ScriptedHook{ .decisions = &.{.{ .extend = clock.nowMillis() + 60_000 }} };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };

    const Writer = struct {
        fn run(write_fd: std.posix.fd_t) void {
            common.sleepNanos(std.time.ns_per_s);
            pipe_proto.writeFrame(write_fd, .result, "{\"exit_ok\":true}") catch {};
            pipe_proto.testOsClose(write_fd);
        }
    };
    var wt = try std.Thread.spawn(.{}, Writer.run, .{fds[1]});
    defer wt.join();

    const near_dl = clock.nowMillis() + 500;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], near_dl, sink, NoopMem.sink(), hook, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(!outcome.timed_out);
    try std.testing.expect(!outcome.terminated);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);
}

test "readResult services a credential_request via the mint hook and frames the token back (§3)" {
    // The parent half of the on-demand mint channel: a `credential_request` frame
    // arrives on the child's stdout; the read loop invokes the mint hook and frames
    // a `credential_response` carrying the token down the child's stdin (here a
    // second pipe the test reads). The trailing `result` frame ends the loop.
    const out = try pipe_proto.testOsPipe(); // child→parent stdout: parent reads out[0]
    defer pipe_proto.testOsClose(out[0]);
    const resp = try pipe_proto.testOsPipe(); // parent→child stdin: test reads resp[0]
    defer pipe_proto.testOsClose(resp[0]);

    try pipe_proto.writeFrame(out[1], .credential_request, "{\"integration\":\"github\"}");
    try pipe_proto.writeFrame(out[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(out[1]);

    // Fake broker forward: any ask mints a fixed token (alloc-owned; the read loop
    // frees it after framing — a leak here would trip the testing allocator).
    const FakeMint = struct {
        fn onMint(_: *anyopaque, alloc: std.mem.Allocator, integration: []const u8, _: ?[]const u8) supervisor.CredentialOutcome {
            std.testing.expectEqualStrings("github", integration) catch return .rejected;
            const tok = alloc.dupe(u8, "ghs_minted") catch return .rejected;
            return .{ .minted = .{ .token = tok, .expires_at_ms = 4242 } };
        }
    };
    var dummy: u8 = 0;
    const mint_hook = supervisor.MintHook{ .ctx = &dummy, .onMint = FakeMint.onMint };
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };

    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, out[0], resp[1], dl, sink, NoopMem.sink(), null, mint_hook);
    defer std.testing.allocator.free(outcome.bytes);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);

    // The token came back framed as a credential_response on the child's stdin.
    pipe_proto.testOsClose(resp[1]);
    const reply = try pipe_proto.readFrame(std.testing.allocator, resp[0], dl, 4096);
    defer std.testing.allocator.free(reply.frame.payload);
    try std.testing.expectEqual(pipe_proto.FrameType.credential_response, reply.frame.ftype);
    const parsed = try std.json.parseFromSlice(cred.PipeResponse, std.testing.allocator, reply.frame.payload, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value.ok);
    try std.testing.expectEqualStrings("ghs_minted", parsed.value.token);
    try std.testing.expectEqual(@as(i64, 4242), parsed.value.expires_at_ms);
}

test "readResult rejects a credential_request when no mint hook is configured (§3)" {
    // A null hook (mint unconfigured) frames ok=false; the child fails closed.
    const out = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(out[0]);
    const resp = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(resp[0]);
    try pipe_proto.writeFrame(out[1], .credential_request, "{\"integration\":\"github\"}");
    try pipe_proto.writeFrame(out[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(out[1]);

    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };
    const dl = clock.nowMillis() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, out[0], resp[1], dl, sink, NoopMem.sink(), null, null);
    defer std.testing.allocator.free(outcome.bytes);

    pipe_proto.testOsClose(resp[1]);
    const reply = try pipe_proto.readFrame(std.testing.allocator, resp[0], dl, 4096);
    defer std.testing.allocator.free(reply.frame.payload);
    const parsed = try std.json.parseFromSlice(cred.PipeResponse, std.testing.allocator, reply.frame.payload, .{});
    defer parsed.deinit();
    try std.testing.expect(!parsed.value.ok);
}

test "sandbox setup fails closed: dev_none runs bare, a required tier with no domain refuses" {
    // dev_none = explicit no-isolation. Every other tier MUST establish its
    // domain or the lease is refused unrun (never executed unsandboxed). The
    // non-Linux arm is here; the Linux cgroup-failure arm is in test-integration.
    try std.testing.expect((try supervisor.establishSandbox(common.globalIo(), std.testing.allocator, false)) == null);
    if (builtin.os.tag != .linux)
        try std.testing.expectError(error.SandboxUnavailable, supervisor.establishSandbox(common.globalIo(), std.testing.allocator, true));
    try std.testing.expect(client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED.len > 0);
}

test "classify: a renewal terminate is renewal_terminate, distinct from a deadline timeout_kill" {
    // Both branches return before touching `scope`, so a null scope is safe and
    // keeps this a pure outcome→category check (no cgroup needed).
    var scope: ?cgroup = null;

    // A renewal `.terminate` (lease lost / capped / no credits) → policy stop.
    const terminated = supervisor.classify(std.testing.allocator, .{ .terminated = true }, .{ .exited = 0 }, &scope);
    try std.testing.expect(!terminated.exit_ok);
    try std.testing.expectEqual(FailureClass.renewal_terminate, terminated.failure.?);

    // A wall-clock deadline elapse → clock stop, a *different* category.
    const timed_out = supervisor.classify(std.testing.allocator, .{ .timed_out = true }, .{ .exited = 0 }, &scope);
    try std.testing.expectEqual(FailureClass.timeout_kill, timed_out.failure.?);

    // The whole point of the fix: the two no longer collapse together.
    try std.testing.expect(terminated.failure.? != timed_out.failure.?);
}

test "classify: a policy terminate outranks a co-occurring deadline timeout" {
    var scope: ?cgroup = null;
    // Deadline elapsed AND the renewal said terminate — the policy reason is the
    // more actionable cause, so terminate wins.
    const both = supervisor.classify(std.testing.allocator, .{ .terminated = true, .timed_out = true }, .{ .exited = 0 }, &scope);
    try std.testing.expectEqual(FailureClass.renewal_terminate, both.failure.?);
}

test "classify: a fleet-budget terminate reports budget_breach, not renewal_terminate" {
    var scope: ?cgroup = null;
    // The refusal named the fleet's own ceiling. That class must survive to the
    // durable `failure_label` — an operator asking "did my budget hold?" reads
    // this, and `renewal_terminate` would answer the wrong question.
    const budget = supervisor.classify(
        std.testing.allocator,
        .{ .terminated = true, .terminate_reason = .budget_breach },
        .{ .exited = 0 },
        &scope,
    );
    try std.testing.expect(!budget.exit_ok);
    try std.testing.expectEqual(FailureClass.budget_breach, budget.failure.?);

    // It still outranks a co-occurring timeout, like any other policy stop.
    const with_timeout = supervisor.classify(
        std.testing.allocator,
        .{ .terminated = true, .timed_out = true, .terminate_reason = .budget_breach },
        .{ .exited = 0 },
        &scope,
    );
    try std.testing.expectEqual(FailureClass.budget_breach, with_timeout.failure.?);
}

test "classify: an unset terminate_reason defaults to renewal_terminate" {
    var scope: ?cgroup = null;
    // Every pre-existing `.terminated = true` call site omits the reason; the
    // field's default keeps their behaviour byte-identical.
    const defaulted = supervisor.classify(std.testing.allocator, .{ .terminated = true }, .{ .exited = 0 }, &scope);
    try std.testing.expectEqual(FailureClass.renewal_terminate, defaulted.failure.?);
}

test "classify maps distinct child exit codes (sandbox-fail, seccomp) to their failure classes" {
    // The child uses distinct exit codes for fail-closed sandbox outcomes so the
    // parent can attribute them instead of lumping all non-zero exits into a
    // crash: SANDBOX_FAIL_EXIT (setup abort — no_new_privs / Landlock / seccomp
    // establish / missing workspace) → startup_posture, matching the parent-side
    // refusals; SECCOMP_VIOLATION_EXIT (a denylisted syscall at run time) →
    // landlock_deny. Both return before touching `scope`, so null is safe.
    var scope: ?cgroup = null;
    const sandbox_fail = supervisor.classify(std.testing.allocator, .{}, .{ .exited = pipe_proto.SANDBOX_FAIL_EXIT }, &scope);
    try std.testing.expect(!sandbox_fail.exit_ok);
    try std.testing.expectEqual(FailureClass.startup_posture, sandbox_fail.failure.?);

    const seccomp_violation = supervisor.classify(std.testing.allocator, .{}, .{ .exited = pipe_proto.SECCOMP_VIOLATION_EXIT }, &scope);
    try std.testing.expectEqual(FailureClass.landlock_deny, seccomp_violation.failure.?);

    // Regression guard on the split: any OTHER non-zero exit is still a crash.
    const other_nonzero = supervisor.classify(std.testing.allocator, .{}, .{ .exited = 1 }, &scope);
    try std.testing.expectEqual(FailureClass.runner_crash, other_nonzero.failure.?);
}

test "classify threads the child's split counts through the result fold" {
    // Regression pin: parseResult must copy the splits off the child's result
    // JSON — dropping them folds the report back to zero and under-bills.
    var scope: ?cgroup = null;
    var body = "{\"exit_ok\":true,\"token_count\":17,\"input_tokens\":10,\"cached_input_tokens\":2,\"output_tokens\":5}".*;
    const r = supervisor.classify(std.testing.allocator, .{ .bytes = &body }, .{ .exited = 0 }, &scope);
    defer std.testing.allocator.free(r.content);
    try std.testing.expect(r.exit_ok);
    try std.testing.expectEqual(@as(u64, 17), r.token_count);
    try std.testing.expectEqual(@as(u64, 10), r.input_tokens);
    try std.testing.expectEqual(@as(u64, 2), r.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 5), r.output_tokens);
}

test "classify parses an old-wire result without splits to zeros (run-fee-only, never an error)" {
    var scope: ?cgroup = null;
    var body = "{\"exit_ok\":true,\"token_count\":17}".*;
    const r = supervisor.classify(std.testing.allocator, .{ .bytes = &body }, .{ .exited = 0 }, &scope);
    defer std.testing.allocator.free(r.content);
    try std.testing.expect(r.exit_ok);
    try std.testing.expectEqual(@as(u64, 17), r.token_count); // legacy total survives
    try std.testing.expectEqual(@as(u64, 0), r.input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.output_tokens);
}
