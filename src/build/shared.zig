//! SharedDeps — the module set BOTH build graphs link (build.zig → agentsfleetd,
//! build_runner.zig → agentsfleet-runner), constructed ONCE here so the server
//! and the runner cannot drift on the shared surface. `pg`/`s3`/`httpz` are
//! deliberately ABSENT — they are daemon-only, and keeping them out of this file
//! is what holds the runner's zero-datastore isolation boundary.
//!
//! Also single-sources the version/git-commit build options both binaries embed.
//! The git SHA is the canonical build identity (CI-passed, always correct);
//! semver `version` rides as a non-gating display label that WARNS (never fails)
//! on a VERSION read error, so a missed bump never costs us build identity.

const std = @import("std");

// NullClaw engine selection — both graphs pass these verbatim (RULE UFS): base
// engines + sqlite for per-run memory, no chat channels (agentsfleet runs agents
// programmatically).
const NULLCLAW_CHANNELS = "none";
const NULLCLAW_ENGINES = "base,sqlite";

// Module import names used internally below — must match the module's `@import`
// sites (the binding names are applied by each graph's `.imports`).
const S_COMMON = "common";
const S_LOG = "log";
const S_NULLCLAW = "nullclaw";

// Version + git-commit build options (consumed via `@import("build_options")`).
const OPT_VERSION = "version";
const OPT_GIT_COMMIT = "git_commit";
const GIT_COMMIT_FLAG = "git-commit";
const GIT_COMMIT_DESC = "Git commit SHA embedded in the binary (passed from CI via GITHUB_SHA)";
const GIT_COMMIT_DEFAULT = "unknown";
const VERSION_FALLBACK = "0.0.0";

/// The modules both build graphs link. Built once via `init`; `pg`/`s3` are
/// intentionally not here (daemon-only — the runner isolation boundary).
pub const SharedDeps = struct {
    log: *std.Build.Module,
    protocol: *std.Build.Module,
    common: *std.Build.Module,
    call_deadline: *std.Build.Module,
    nullclaw: *std.Build.Module,

    pub fn init(
        b: *std.Build,
        target: std.Build.ResolvedTarget,
        optimize: std.builtin.OptimizeMode,
    ) SharedDeps {
        // common: pure, datastore-free lease/runner knobs (src/lib/common). A
        // named module because src/lib sits outside each graph's module root.
        const common = b.createModule(.{
            .root_source_file = b.path("src/lib/common/constants.zig"),
        });

        // log: structured logging (src/lib/logging). Sources its envelope
        // wall-clock from `common.clock` (Zig 0.16 removed std.time.*Timestamp);
        // `common` is datastore-free, so this adds no domain coupling and no
        // cycle (common never imports log).
        const log = b.createModule(.{
            .root_source_file = b.path("src/lib/logging/mod.zig"),
        });
        log.addImport(S_COMMON, common);

        // protocol: the frozen `/v1/runners` wire protocol (the src/lib/contract
        // module — legacy name, pending a dedicated rename). One source, two
        // graphs, so the server and the client cannot drift.
        const protocol = b.createModule(.{
            .root_source_file = b.path("src/lib/contract/contract.zig"),
        });

        // call_deadline: the socket-shutdown call watchdog + the runner's
        // per-verb deadline policy (src/lib/call_deadline). Bounds outbound
        // HTTP in both binaries: the runner's control-plane verbs and the
        // daemon's connector vendor calls (bounded_fetch). Datastore-free.
        const call_deadline = b.createModule(.{
            .root_source_file = b.path("src/lib/call_deadline/call_deadline.zig"),
        });
        call_deadline.addImport(S_COMMON, common);
        call_deadline.addImport(S_LOG, log);

        // NullClaw engine dependency — same options on both graphs (RULE UFS).
        const nullclaw_dep = b.dependency(S_NULLCLAW, .{
            .target = target,
            .optimize = optimize,
            .channels = @as([]const u8, NULLCLAW_CHANNELS),
            .engines = @as([]const u8, NULLCLAW_ENGINES),
        });

        return .{
            .log = log,
            .protocol = protocol,
            .common = common,
            .call_deadline = call_deadline,
            .nullclaw = nullclaw_dep.module(S_NULLCLAW),
        };
    }
};

/// The git SHA (CI-passed via `-Dgit-commit`) — the canonical build identity.
/// Call once per graph.
pub fn resolveGitCommit(b: *std.Build) []const u8 {
    return b.option([]const u8, GIT_COMMIT_FLAG, GIT_COMMIT_DESC) orelse GIT_COMMIT_DEFAULT;
}

/// The semver from the repo VERSION file — a non-gating display label. WARNS
/// (never silently) on a read failure so a missing/renamed VERSION is visible at
/// build time; the git SHA still uniquely identifies the build.
pub fn resolveVersion(b: *std.Build) []const u8 {
    const raw = b.build_root.handle.readFileAlloc(b.graph.io, "VERSION", b.allocator, .limited(64)) catch |err| {
        std.log.warn("build: VERSION unreadable ({s}) — version falls back to \"{s}\"; git SHA remains the canonical identity", .{ @errorName(err), VERSION_FALLBACK });
        return VERSION_FALLBACK;
    };
    return std.mem.trim(u8, raw, " \t\r\n");
}

/// Embed the `version` + `git_commit` pair onto a build Options — identical
/// wiring in both graphs.
pub fn addVersionOptions(opts: *std.Build.Step.Options, version: []const u8, git_commit: []const u8) void {
    opts.addOption([]const u8, OPT_VERSION, version);
    opts.addOption([]const u8, OPT_GIT_COMMIT, git_commit);
}
