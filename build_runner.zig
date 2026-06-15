//! Dedicated build graph for the host-resident `agentsfleet-runner` daemon.
//!
//! Separate from the root `build.zig` (which builds `agentsfleetd`) by design:
//! the runner holds zero datastore credentials and links no server
//! infrastructure — it can only ever depend on what is imported here, and
//! `pg` / `httpz` / `redis` are deliberately absent. The runner is a
//! long-running daemon and an HTTP *client* of `agentsfleetd` (it long-polls
//! `POST /v1/runners/me/leases`, runs the event, reports, loops) — it serves
//! no inbound HTTP, so it has no router or handlers of its own.
//!
//! The frozen wire protocol (`src/lib/contract`) is shared with `agentsfleetd` as a
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

// Build-option names + the runner root, single-sourced (RULE UFS) — each is
// referenced by the prod, stub-exe, and integration options modules below.
const OPT_VERSION = "version";
const OPT_GIT_COMMIT = "git_commit";
const OPT_EXECUTOR_PROVIDER_STUB = "executor_provider_stub";
const OPT_STUB_RUNNER_EXE_PATH = "stub_runner_exe_path";
const SRC_RUNNER_MAIN = "src/runner/main.zig";

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Logfmt logging, shared by source with agentsfleetd.
    const log_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/logging/mod.zig"),
    });

    // The shared `/v1/runners` wire contract (src/lib/contract), reached as a
    // named module — the runner's ONLY shared surface beyond `log`. This is the
    // entire reason it compiles the protocol without crossing into src/agentsfleetd/.
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

    // Logging sources its envelope wall-clock from `common.clock` (Zig 0.16
    // removed std.time.*Timestamp); `common` is pure/datastore-free so the
    // runner's zero-credential invariant holds and there is no import cycle.
    log_mod.addImport(S_COMMON, common_mod);

    // NullClaw engine dependency — same options as the agentsfleetd build graph.
    const nullclaw_dep = b.dependency(S_NULLCLAW, .{
        .target = target,
        .optimize = optimize,
        .channels = @as([]const u8, "none"),
        .engines = @as([]const u8, "base,sqlite"),
    });
    const nullclaw_mod = nullclaw_dep.module(S_NULLCLAW);

    // Build options. `version` (read from the repo VERSION file, kept in sync by
    // `make sync-version`) + `git_commit` (-Dgit-commit, passed from CI) back
    // `--version` — the same git-commit knob agentsfleetd's build.zig exposes (RULE
    // UFS). `executor_provider_stub` (default false) is a TEST-ONLY build flag: a
    // child built with it emits a canned `result` frame instead of running the
    // NullClaw engine, and a daemon built with it redirects the forked child's
    // exec target to `stub_runner_exe_path` — so the worker-pool integration lane
    // can drive the real lease→fork→execute→report path with no LLM. Production
    // builds leave it false, so both seams comptime-vanish (no env backdoor).
    const git_commit = b.option([]const u8, "git-commit", "Git commit SHA embedded in the binary (passed from CI via GITHUB_SHA)") orelse "unknown";
    const version_raw = b.build_root.handle.readFileAlloc(b.graph.io, "VERSION", b.allocator, .limited(64)) catch "0.0.0";
    const version = std.mem.trim(u8, version_raw, " \t\r\n");

    // Production options (exe + unit tests): real engine, no exec redirect.
    const build_opts = b.addOptions();
    build_opts.addOption([]const u8, OPT_VERSION, version);
    build_opts.addOption([]const u8, OPT_GIT_COMMIT, git_commit);
    build_opts.addOption(bool, OPT_EXECUTOR_PROVIDER_STUB, false);
    build_opts.addOption([]const u8, OPT_STUB_RUNNER_EXE_PATH, "");
    const build_options_mod = build_opts.createModule();

    const runner_exe = b.addExecutable(.{
        .name = "agentsfleet-runner",
        .root_module = b.createModule(.{
            .root_source_file = b.path(SRC_RUNNER_MAIN),
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
    b.step("run", "Run the agentsfleet-runner daemon").dependOn(&run_cmd.step);

    // Runner-side test target — `zig build --build-file build_runner.zig test`
    // (the `test-unit-agentsfleet-runner` make target). Same root + module wiring as the
    // exe, so it proves exactly what ships and links no datastore: a red agentsfleetd
    // (`src/`) suite never blocks building, testing, or shipping the runner.
    const runner_tests = b.addTest(.{
        .name = "agentsfleet-runner-tests",
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
    b.step("test", "Run agentsfleet-runner unit tests (contract + daemon + common)").dependOn(&b.addRunArtifact(runner_tests).step);

    // The stub child binary the worker-pool integration lane forks: a real
    // `agentsfleet-runner` built with `executor_provider_stub=true`, so its `__execute`
    // mode emits a canned `result` frame with no engine/LLM. The integration test
    // binary can't be the child itself (a `zig test` binary has no `__execute`
    // dispatch), so the harness points the forked child at THIS artifact's path.
    const stub_exe_opts = b.addOptions();
    stub_exe_opts.addOption([]const u8, OPT_VERSION, version);
    stub_exe_opts.addOption([]const u8, OPT_GIT_COMMIT, git_commit);
    stub_exe_opts.addOption(bool, OPT_EXECUTOR_PROVIDER_STUB, true);
    stub_exe_opts.addOption([]const u8, OPT_STUB_RUNNER_EXE_PATH, "");
    const stub_runner_exe = b.addExecutable(.{
        .name = "agentsfleet-runner-execstub",
        .root_module = b.createModule(.{
            .root_source_file = b.path(SRC_RUNNER_MAIN),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = S_BUILD_OPTIONS, .module = stub_exe_opts.createModule() },
            },
        }),
    });

    // Integration-test options: stub flag ON (so the daemon-side `buildArgv`
    // redirects the child exec target) + the stub exe's built path. `addOptionPath`
    // makes the test compilation depend on the stub exe being emitted first.
    const integ_opts = b.addOptions();
    integ_opts.addOption([]const u8, OPT_VERSION, version);
    integ_opts.addOption([]const u8, OPT_GIT_COMMIT, git_commit);
    integ_opts.addOption(bool, OPT_EXECUTOR_PROVIDER_STUB, true);
    integ_opts.addOptionPath(OPT_STUB_RUNNER_EXE_PATH, stub_runner_exe.getEmittedBin());

    // Runner integration tests — real-process proofs (fork/spawn at the real
    // environ_map boundary, kill(-pgid) tree reap, and the worker pool running N
    // real forked children concurrently). Rooted separately from the unit `test`
    // step so the fast unit lane never forks real children. The bodies are
    // Linux-only (SkipZigTest elsewhere); the `test-integration-agentsfleet-runner` make lane
    // runs them on a Linux host.
    const runner_integration_tests = b.addTest(.{
        .name = "agentsfleet-runner-integration-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/runner/sandbox_integration_test.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = S_BUILD_OPTIONS, .module = integ_opts.createModule() },
            },
        }),
    });
    b.step("test-integration", "Run agentsfleet-runner integration tests (real-process sandbox proofs + worker-pool concurrency, Linux)").dependOn(&b.addRunArtifact(runner_integration_tests).step);
}
