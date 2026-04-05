//! M28_001 §1.4: Gate tool OTel span emission.
//! Separated from worker_gate_loop.zig for the 500-line limit.

const std = @import("std");
const otel_traces = @import("../observability/otel_traces.zig");
const trace_mod = @import("../observability/trace.zig");

pub const GateSpanContext = struct {
    run_id: []const u8,
    workspace_id: []const u8,
    trace_id: []const u8,
    root_span_id: [trace_mod.SPAN_ID_HEX_LEN]u8,
};

pub const GateSpanResult = struct {
    gate_name: []const u8,
    exit_code: u32,
    wall_ms: u64,
    passed: bool,
};

/// Emit a gate tool span as a child of the root run span.
pub fn emit(gsc: GateSpanContext, result: GateSpanResult, repair_attempt: u32) void {
    if (gsc.trace_id.len == 0) return;
    var tc: trace_mod.TraceContext = undefined;
    const tid_len = @min(gsc.trace_id.len, trace_mod.TRACE_ID_HEX_LEN);
    @memcpy(tc.trace_id[0..tid_len], gsc.trace_id[0..tid_len]);
    if (tid_len < trace_mod.TRACE_ID_HEX_LEN) @memset(tc.trace_id[tid_len..], '0');
    const child = trace_mod.TraceContext.generate();
    tc.span_id = child.span_id;
    tc.parent_span_id = gsc.root_span_id;

    const now_ns: u64 = @intCast(std.time.nanoTimestamp());
    const start_ns = if (now_ns > result.wall_ms * std.time.ns_per_ms)
        now_ns - result.wall_ms * std.time.ns_per_ms
    else
        now_ns;

    var span = otel_traces.buildSpan(tc, "gate.check", start_ns, now_ns);
    _ = otel_traces.addAttr(&span, "run.id", gsc.run_id);
    _ = otel_traces.addAttr(&span, "workspace.id", gsc.workspace_id);
    _ = otel_traces.addAttr(&span, "gate.name", result.gate_name);
    var attempt_buf: [10]u8 = undefined;
    const attempt_str = std.fmt.bufPrint(&attempt_buf, "{d}", .{repair_attempt}) catch "0";
    _ = otel_traces.addAttr(&span, "gate.attempt", attempt_str);
    var exit_buf: [10]u8 = undefined;
    const exit_str = std.fmt.bufPrint(&exit_buf, "{d}", .{result.exit_code}) catch "0";
    _ = otel_traces.addAttr(&span, "gate.exit_code", exit_str);
    var dur_buf: [20]u8 = undefined;
    const dur_str = std.fmt.bufPrint(&dur_buf, "{d}", .{result.wall_ms}) catch "0";
    _ = otel_traces.addAttr(&span, "gate.duration_ms", dur_str);
    _ = otel_traces.addAttr(&span, "gate.passed", if (result.passed) "true" else "false");
    otel_traces.enqueueSpan(span);
}
