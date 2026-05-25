//! Dedicated build graph for the host-resident `zombie-runner` daemon.
//!
//! Separate from the root `build.zig` (which builds `zombied` + the executor)
//! by design: the runner holds zero datastore credentials and links no server
//! infrastructure — it can only ever depend on what is imported here, and
//! `pg` / `httpz` / `redis` are deliberately absent. The runner is a
//! long-running daemon and an HTTP *client* of `zombied` (it long-polls
//! `POST /v1/runners/me/leases`, runs the event, reports, loops) — it serves
//! no inbound HTTP, so it has no router or handlers of its own.
//!
//! The frozen wire protocol (`src/runner/protocol.zig`) is shared with
//! `zombied` by referencing the same source, so the server and the client
//! cannot drift. The keystone skeleton built here logs one health line and
//! exits 0; the lease loop + folded-in executor land later.
//!
//! Build:  zig build --build-file build_runner.zig
//! Run:    zig build --build-file build_runner.zig run

const std = @import("std");

const S_LOG = "log";

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Logfmt logging, shared by source with zombied + the executor sidecar.
    const log_mod = b.createModule(.{
        .root_source_file = b.path("src/logging/mod.zig"),
    });

    const runner_exe = b.addExecutable(.{
        .name = "zombie-runner",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/runner/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_LOG, .module = log_mod },
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
}
