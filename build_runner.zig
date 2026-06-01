//! Dedicated build graph for the host-resident `zombie-runner` daemon.
//!
//! Separate from the root `build.zig` (which builds `zombied`) by design:
//! the runner holds zero datastore credentials and links no server
//! infrastructure — it can only ever depend on what is imported here, and
//! `pg` / `httpz` / `redis` are deliberately absent. The runner is a
//! long-running daemon and an HTTP *client* of `zombied` (it long-polls
//! `POST /v1/runners/me/leases`, runs the event, reports, loops) — it serves
//! no inbound HTTP, so it has no router or handlers of its own.
//!
//! The frozen wire protocol (`src/lib/contract`) is shared with `zombied` as a
//! named module (`@import("contract")`) — one source, two build graphs — so the
//! server and the client cannot drift. `src/runner/main.zig` runs the real
//! register → heartbeat → lease → forked-sandboxed-child → report loop, with
//! the NullClaw engine folded in (`src/runner/engine`) and no datastore linked.
//!
//! Build:  zig build --build-file build_runner.zig
//! Run:    zig build --build-file build_runner.zig run

const std = @import("std");

const S_LOG = "log";
const S_CONTRACT = "contract";
const S_COMMON = "common";
const S_NULLCLAW = "nullclaw";
const S_BUILD_OPTIONS = "build_options";

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Logfmt logging, shared by source with zombied.
    const log_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/logging/mod.zig"),
    });

    // The shared `/v1/runners` wire contract (src/lib/contract), reached as a
    // named module — the runner's ONLY shared surface beyond `log`. This is the
    // entire reason it compiles the protocol without crossing into src/zombied/.
    // `pg`/`httpz`/`redis` remain deliberately absent.
    const contract_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/contract/contract.zig"),
    });

    // Single-source lease/runner knobs (src/lib/common) both binaries key off
    // (RULE UFS); datastore-free, so importing it keeps the zero-credential
    // invariant. A named module because src/lib sits outside the runner root.
    const common_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/common/constants.zig"),
    });

    // NullClaw engine dependency — same options as the zombied build graph.
    const nullclaw_dep = b.dependency(S_NULLCLAW, .{
        .target = target,
        .optimize = optimize,
        .channels = @as([]const u8, "none"),
        .engines = @as([]const u8, "base,sqlite"),
    });
    const nullclaw_mod = nullclaw_dep.module(S_NULLCLAW);

    // Build options shared by both exe and tests. harness/stub flags are always
    // false (the runner always runs the real engine). `version` (read from the
    // repo VERSION file, kept in sync by `make sync-version`) + `git_commit`
    // (-Dgit-commit, passed from CI) back `--version` — the same git-commit knob
    // zombied's build.zig exposes (RULE UFS).
    const git_commit = b.option([]const u8, "git-commit", "Git commit SHA embedded in the binary (passed from CI via GITHUB_SHA)") orelse "unknown";
    const version_raw = b.build_root.handle.readFileAlloc(b.allocator, "VERSION", 64) catch "0.0.0";
    const version = std.mem.trim(u8, version_raw, " \t\r\n");
    const build_opts = b.addOptions();
    build_opts.addOption(bool, "executor_harness", false);
    build_opts.addOption(bool, "executor_provider_stub", false);
    build_opts.addOption([]const u8, "version", version);
    build_opts.addOption([]const u8, "git_commit", git_commit);
    const build_options_mod = build_opts.createModule();

    const runner_exe = b.addExecutable(.{
        .name = "zombie-runner",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/runner/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = S_BUILD_OPTIONS, .module = build_options_mod },
            },
        }),
    });
    if (optimize == .ReleaseSmall) {
        runner_exe.root_module.strip = true;
    }
    b.installArtifact(runner_exe);

    const run_cmd = b.addRunArtifact(runner_exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    b.step("run", "Run the zombie-runner daemon").dependOn(&run_cmd.step);

    // Runner-side test target — `zig build --build-file build_runner.zig test`
    // (the `test-unit-zigrunner` make target). Same root + module wiring as the
    // exe, so it proves exactly what ships and links no datastore: a red zombied
    // (`src/`) suite never blocks building, testing, or shipping the runner.
    const runner_tests = b.addTest(.{
        .name = "zombie-runner-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/runner/tests.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = S_BUILD_OPTIONS, .module = build_options_mod },
            },
        }),
    });
    b.step("test", "Run zombie-runner unit tests (contract + daemon + common)").dependOn(&b.addRunArtifact(runner_tests).step);
}
