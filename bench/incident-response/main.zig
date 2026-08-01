//! bench-incident entrypoint. Prints the corpus hash (the reproducibility
//! line the release rubric greps), optionally writes the corpus for the demo
//! playbook, and scores a findings file against the frozen baseline. Every
//! refusal — violated split, drifted baseline, corpus-hash mismatch — exits
//! nonzero naming its cause; the hash mismatch names both hashes.

const std = @import("std");
const manifest = @import("manifest.zig");
const injector = @import("injector.zig");
const baseline = @import("baseline.zig");
const scoring = @import("scoring.zig");
const report = @import("report.zig");

const MAX_INPUT_BYTES = 4 * 1024 * 1024;
const EXIT_REFUSED: u8 = 1;

const USAGE =
    "usage: bench-incident --evaluation <path> --calibration <path>\n" ++
    "         [--baseline <path> --freeze <path>] [--runs <path>]\n" ++
    "         [--corpus-out <path>] [--print-baseline-hash]\n";

const Args = struct {
    evaluation: ?[]const u8 = null,
    calibration: ?[]const u8 = null,
    baseline: ?[]const u8 = null,
    freeze: ?[]const u8 = null,
    runs: ?[]const u8 = null,
    corpus_out: ?[]const u8 = null,
    print_baseline_hash: bool = false,
};

pub fn main(init: std.process.Init) !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();
    const io = init.io;

    var stdout_buf: [4096]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &stdout_buf);
    const out = &stdout_w.interface;

    const argv = try init.minimal.args.toSlice(init.arena.allocator());
    const args = parseArgs(argv) orelse {
        try out.writeAll(USAGE);
        try out.flush();
        std.process.exit(EXIT_REFUSED);
    };
    run(alloc, io, out, args) catch |err| {
        try out.print("bench-incident: refused: {s}\n", .{@errorName(err)});
        try out.flush();
        std.process.exit(EXIT_REFUSED);
    };
    try out.flush();
}

fn parseArgs(argv: []const [:0]const u8) ?Args {
    var args: Args = .{};
    var i: usize = 1;
    while (i < argv.len) : (i += 1) {
        const flag = argv[i];
        if (std.mem.eql(u8, flag, "--print-baseline-hash")) {
            args.print_baseline_hash = true;
            continue;
        }
        if (i + 1 >= argv.len) return null;
        i += 1;
        const value = argv[i];
        if (std.mem.eql(u8, flag, "--evaluation")) {
            args.evaluation = value;
        } else if (std.mem.eql(u8, flag, "--calibration")) {
            args.calibration = value;
        } else if (std.mem.eql(u8, flag, "--baseline")) {
            args.baseline = value;
        } else if (std.mem.eql(u8, flag, "--freeze")) {
            args.freeze = value;
        } else if (std.mem.eql(u8, flag, "--runs")) {
            args.runs = value;
        } else if (std.mem.eql(u8, flag, "--corpus-out")) {
            args.corpus_out = value;
        } else {
            return null;
        }
    }
    return args;
}

fn run(alloc: std.mem.Allocator, io: std.Io, out: *std.Io.Writer, args: Args) !void {
    if (args.print_baseline_hash) {
        const b = try loadBaseline(alloc, io, args.baseline orelse return error.MissingBaselinePath);
        defer b.deinit();
        const hash = baseline.configHashHex(b.value);
        try out.print("baseline_config_hash={s}\n", .{&hash});
        return;
    }

    const eval_raw = try readInput(alloc, io, args.evaluation orelse return error.MissingEvaluationPath);
    defer alloc.free(eval_raw);
    const calib_raw = try readInput(alloc, io, args.calibration orelse return error.MissingCalibrationPath);
    defer alloc.free(calib_raw);
    const eval_m = try manifest.parse(alloc, eval_raw);
    defer eval_m.deinit();
    const calib_m = try manifest.parse(alloc, calib_raw);
    defer calib_m.deinit();
    try manifest.assertDisjoint(eval_m.value, calib_m.value);

    const corpus_hash = try injector.corpusHashHex(eval_m.value);
    try out.print("corpus_hash={s}\n", .{&corpus_hash});
    if (args.corpus_out) |path| try injector.writeCorpus(io, eval_m.value, path);

    const runs_path = args.runs orelse return;
    try scoreRuns(alloc, io, out, args, eval_m.value, calib_m.value, &corpus_hash, runs_path);
}

fn scoreRuns(
    alloc: std.mem.Allocator,
    io: std.Io,
    out: *std.Io.Writer,
    args: Args,
    eval_m: manifest.SeedManifest,
    calib_m: manifest.SeedManifest,
    corpus_hash: []const u8,
    runs_path: []const u8,
) !void {
    const runs_raw = try readInput(alloc, io, runs_path);
    defer alloc.free(runs_raw);
    const run_set = try scoring.parseRunSet(alloc, runs_raw);
    defer run_set.deinit();
    if (!std.mem.eql(u8, run_set.value.corpus_hash, corpus_hash)) {
        try out.print("bench-incident: refusing to score: findings corpus_hash={s} but manifest renders corpus_hash={s}\n", .{
            run_set.value.corpus_hash, corpus_hash,
        });
        return error.CorpusHashMismatch;
    }

    const b = try loadBaseline(alloc, io, args.baseline orelse return error.MissingBaselinePath);
    defer b.deinit();
    const freeze_raw = try readInput(alloc, io, args.freeze orelse return error.MissingFreezePath);
    defer alloc.free(freeze_raw);
    const freeze = try baseline.parseFreeze(alloc, freeze_raw);
    defer freeze.deinit();

    var r = try scoring.score(alloc, eval_m, calib_m, b.value, freeze.value, run_set.value.runs);
    defer r.deinit(alloc);
    const json = try report.emitJson(alloc, r);
    defer alloc.free(json);
    try out.writeAll(json);
    try out.writeAll("\n");
}

fn readInput(alloc: std.mem.Allocator, io: std.Io, path: []const u8) ![]u8 {
    return std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(MAX_INPUT_BYTES));
}

fn loadBaseline(alloc: std.mem.Allocator, io: std.Io, path: []const u8) !std.json.Parsed(baseline.Baseline) {
    const raw = try readInput(alloc, io, path);
    defer alloc.free(raw);
    return baseline.parseBaseline(alloc, raw);
}

test {
    _ = @import("bench_incident_test.zig");
}
