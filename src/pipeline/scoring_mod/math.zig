const std = @import("std");
const types = @import("types.zig");

pub fn computeCompletionScore(outcome: types.TerminalOutcome) u8 {
    return switch (outcome) {
        .done => 100,
        .blocked_retries_exhausted => 30,
        .blocked_gate_exhausted => 20,
        .blocked_stage_graph => 10,
        .error_propagation => 0,
        .pending => 0,
    };
}

pub fn computeErrorRateScore(passed: u32, total: u32) u8 {
    if (total == 0) return 0;
    const numerator = @as(u64, passed) * 100 + @as(u64, total) / 2;
    const result = @divFloor(numerator, @as(u64, total));
    return @intCast(@min(result, 100));
}

pub fn computeLatencyScore(wall_seconds: u64, baseline: ?types.LatencyBaseline) u8 {
    const bl = baseline orelse return 50;
    if (bl.sample_count < 5) return 50;
    if (bl.p50_seconds == 0) {
        return if (wall_seconds == 0) 100 else 0;
    }
    if (wall_seconds <= bl.p50_seconds) return 100;

    const range = bl.p50_seconds * 2;
    const excess = wall_seconds - bl.p50_seconds;
    if (excess >= range) return 0;

    return @intCast(100 - @divFloor(excess * 100, range));
}

pub fn computeResourceScore() u8 {
    return 50;
}

pub fn computeScore(axes: types.AxisScores, weights: types.Weights) u8 {
    const raw: f64 =
        @as(f64, @floatFromInt(axes.completion)) * weights.completion +
        @as(f64, @floatFromInt(axes.error_rate)) * weights.error_rate +
        @as(f64, @floatFromInt(axes.latency)) * weights.latency +
        @as(f64, @floatFromInt(axes.resource)) * weights.resource;

    const rounded = @as(i64, @intFromFloat(@round(raw)));
    return @intCast(@as(u64, @intCast(std.math.clamp(rounded, 0, 100))));
}

pub fn tierFromScore(score: ?u8) types.Tier {
    const value = score orelse return .unranked;
    if (value >= 90) return .elite;
    if (value >= 70) return .gold;
    if (value >= 40) return .silver;
    return .bronze;
}

pub fn tierFromRun(score: u8, baseline: ?types.LatencyBaseline) types.Tier {
    if (!types.hasPriorRuns(baseline)) return .unranked;
    if (score >= 90) return .elite;
    if (score >= 70) return .gold;
    if (score >= 40) return .silver;
    return .bronze;
}

pub fn validateWeights(weights: types.Weights) !void {
    if (weights.completion <= 0 or weights.error_rate <= 0 or weights.latency <= 0 or weights.resource <= 0) {
        return error.InvalidScoringWeights;
    }

    const sum = weights.completion + weights.error_rate + weights.latency + weights.resource;
    if (@abs(sum - 1.0) > 0.0001) return error.InvalidScoringWeights;
}

pub fn parseWeightsJson(alloc: std.mem.Allocator, raw: []const u8) !types.Weights {
    const parsed = try std.json.parseFromSlice(types.WeightsDoc, alloc, raw, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();

    const weights = types.Weights{
        .completion = parsed.value.completion orelse types.DEFAULT_WEIGHTS.completion,
        .error_rate = parsed.value.error_rate orelse types.DEFAULT_WEIGHTS.error_rate,
        .latency = parsed.value.latency orelse types.DEFAULT_WEIGHTS.latency,
        .resource = parsed.value.resource orelse types.DEFAULT_WEIGHTS.resource,
    };
    try validateWeights(weights);
    return weights;
}

pub fn axisScoresJson(alloc: std.mem.Allocator, axes: types.AxisScores) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .completion = axes.completion,
        .error_rate = axes.error_rate,
        .latency = axes.latency,
        .resource = axes.resource,
    }, .{});
}

pub fn weightsJson(alloc: std.mem.Allocator, weights: types.Weights) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .completion = weights.completion,
        .error_rate = weights.error_rate,
        .latency = weights.latency,
        .resource = weights.resource,
    }, .{});
}
