const std = @import("std");
const buildpkg = @import("src/build/main.zig");

comptime {
    // Fail fast (with a clear message) if the toolchain drifted from the
    // minimum_zig_version pinned in build.zig.zon.
    buildpkg.requireZig(@import("build.zig.zon").minimum_zig_version);
}

const S_POSTHOG = "posthog";
const S_HTTPZ = "httpz";
const S_CACHE = "cache";
const S_ZBENCH = "zbench";
const S_BUILD_OPTIONS = "build_options";
const S_SCHEMA = "schema";
const S_SRC_MAIN_ZIG = "src/agentsfleetd/main.zig";
const S_AGENTSFLEETD_TESTS_ROOT = "src/agentsfleetd/tests.zig";
const S_NULLCLAW = "nullclaw";
const S_AGENTSFLEETD_TESTS = "agentsfleetd-tests";
const S_LOG = "log";
const S_HMAC_SIG = "hmac_sig";
const S_AUTH_CODES = "auth_codes";
const S_PG = "pg";
const S_YAML = "yaml";
const S_CONTRACT = "contract";
const S_COMMON = "common";
const S_CALL_DEADLINE = "call_deadline";
const S_S3 = "s3";
const S_GEN_ERROR_CODES = "gen-error-codes";
// Directory the daemon test root is rooted at. Zig names a registered test after
// its source path relative to this directory, so `list-tests` echoes it for the
// reachability checker (`scripts/check_zig_test_reachability.py`).
const S_AGENTSFLEETD_TESTS_ROOT_DIR = "src/agentsfleetd";

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const with_bench_tools = b.option(bool, "with-bench-tools", "Enable benchmark tooling (zBench)") orelse false;
    const test_filter = b.option([]const u8, "test-filter", "Restrict Zig tests to names containing this substring");
    const git_commit = buildpkg.shared.resolveGitCommit(b);
    const version = buildpkg.shared.resolveVersion(b);
    const build_opts = b.addOptions();
    // git SHA is the canonical build identity; semver is a non-gating label.
    buildpkg.shared.addVersionOptions(build_opts, version, git_commit);
    // One build_options module, reused by the exe + every test target.
    const build_options_mod = build_opts.createModule();
    const test_filters: []const []const u8 = if (test_filter) |filter| &.{filter} else &.{};

    // ── NullClaw dependency ──────────────────────────────────────────────────
    // Use base engines (sqlite for per-run memory) + no channels (we don't
    // need chat channels — agentsfleet runs agents programmatically).
    const deps = buildpkg.shared.SharedDeps.init(b, target, optimize);
    const nullclaw_mod = deps.nullclaw;

    // ── httpz (pure-Zig HTTP server, karlseguin) ─────────────────────────────
    const httpz_dep = b.dependency(S_HTTPZ, .{
        .target = target,
        .optimize = optimize,
    });
    const httpz_mod = httpz_dep.module(S_HTTPZ);

    // cache.zig (karlseguin) — sharded, RwLock-shared reads, refcounted entries.
    const cache_mod = b.dependency(S_CACHE, .{ .target = target, .optimize = optimize }).module(S_CACHE);

    const pg_mod = buildpkg.pg.module(b, target, optimize, S_PG);

    // ── posthog-zig (server-side PostHog SDK) ───────────────────────────────
    const posthog_dep = b.dependency(S_POSTHOG, .{
        .target = target,
        .optimize = optimize,
    });
    const posthog_mod = posthog_dep.module(S_POSTHOG);

    // ── zig-yaml (TRIGGER.md / SKILL.md frontmatter parsing) ────────────────
    // Pinned to 0.2.0 (Zig 0.15.x compatible). main targets Zig 0.16; do not
    // re-pin without verifying the toolchain. Replaces the bespoke YAML→JSON
    // converter in src/agent/yaml_frontmatter.zig — gains depth-N nesting,
    // duplicate-key detection, and proper YAML 1.2 scalar handling.
    const zig_yaml_dep = b.dependency("zig_yaml", .{
        .target = target,
        .optimize = optimize,
    });
    const yaml_mod = zig_yaml_dep.module(S_YAML);

    // ── Schema embed module (root = schema/ so @embedFile is in-bounds) ──────
    const schema_mod = b.createModule(.{
        .root_source_file = b.path("schema/embed.zig"),
    });

    // ── Crypto primitives module: shared HMAC/CT/hex ─────────────────────────
    // Pure stdlib only; no deps. Importable from src/auth/ without breaking the
    // test-auth portability gate, and from src/agent/ as the canonical source
    // for webhook signature verification primitives.
    const hmac_sig_mod = b.createModule(.{
        .root_source_file = b.path("src/agentsfleetd/crypto/hmac_sig.zig"),
    });

    // Auth-plane error-code mirror leaf — see auth_codes.zig header.
    const auth_codes_mod = b.createModule(.{
        .root_source_file = b.path("src/agentsfleetd/errors/auth_codes.zig"),
    });

    // ── Logging module ───────────────────────────────────────────────────────
    // Shared `log.scoped` API + pretty-printer + fatalStderr per
    // docs/LOGGING_STANDARD.md. Importable from every binary AND from
    // src/auth/ + the runner engine (which would otherwise be portability
    // islands forbidden from reaching across `src/`). Module-named import
    // makes the boundary clean — those layers still cannot import
    // arbitrary cross-layer code, just the canonical logging surface.
    //
    // Lives at src/logging/ — a peer of src/observability/ — because it's
    // strictly the structured-log facility. Wider observability concerns
    // (OTel exporters, metrics, traces) live under src/observability/ and
    // import this module.
    //
    // No domain dependencies (no error_registry import). Callers that
    // need to embed an error_code field in a log record pass it as a
    // struct field (`.{ .error_code = error_codes.ERR_X, ... }`), keeping
    // logging/ pure of business knowledge.
    const log_mod = deps.log;

    // Shared `/v1/runners` wire contract (src/lib/contract). A named module so
    // both build graphs reach it without crossing module boundaries (see
    // docs/ZIG_RULES.md "Module Boundaries & Shared Modules"). No deps — its
    // files import only std + each other within src/lib/contract/.
    const contract_mod = deps.protocol;

    // Single-source lease/runner knobs (src/lib/common) the control plane (fleet)
    // and the runner daemon both key off (RULE UFS). Named module: src/lib sits
    // outside the agentsfleetd module root, so it cannot be relative-imported.
    const common_mod = deps.common;

    // Socket-shutdown call watchdog (src/lib/call_deadline) — bounds the
    // connectors' outbound vendor HTTP (bounded_fetch); shared with the
    // runner's control-plane client so the mechanism exists exactly once.
    const call_deadline_mod = deps.call_deadline;

    // hmac_sig sources its wall-clock from `common.clock` (Zig 0.16 removed
    // std.time.*Timestamp). Same pure, datastore-free shared module as log_mod —
    // no domain coupling, no cycle (common never imports hmac_sig).
    hmac_sig_mod.addImport(S_COMMON, common_mod);

    // R2 (Cloudflare) wrapper for Fleet Bundle snapshots — daemon graph only
    // (the runner holds zero datastore credentials). See src/build/s3.zig.
    const s3_mod = buildpkg.s3.module(b, target, optimize);

    // ── agentsfleet executable ───────────────────────────────────────────────────
    const exe = b.addExecutable(.{
        .name = "agentsfleetd",
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_SRC_MAIN_ZIG),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = S_HTTPZ, .module = httpz_mod },
                .{ .name = S_CACHE, .module = cache_mod },
                .{ .name = S_PG, .module = pg_mod },
                .{ .name = S_POSTHOG, .module = posthog_mod },
                .{ .name = S_SCHEMA, .module = schema_mod },
                .{ .name = S_BUILD_OPTIONS, .module = build_options_mod },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_CALL_DEADLINE, .module = call_deadline_mod },
                .{ .name = S_YAML, .module = yaml_mod },
                .{ .name = S_S3, .module = s3_mod },
            },
        }),
    });

    // Only strip in ReleaseSmall (musl/minimal builds). ReleaseSafe keeps debug
    // info so panics produce usable stack traces in production.
    if (optimize == .ReleaseSmall) {
        exe.root_module.strip = true;
    }

    b.installArtifact(exe);

    // Execution left this build graph at the M80 cutover: the standalone sandbox
    // sidecar (and its harness/stub fixtures) is gone, replaced
    // by the host-resident `agentsfleet-runner` daemon, which has its own build graph
    // (`build_runner.zig`) and never links agentsfleetd's server infrastructure
    // (pg/httpz/redis). It shares only the frozen wire protocol by source.

    // ── Shared src/lib test step (`test-lib`) ────────────────────────────────
    // Extracted to src/build/lib_tests.zig (RULE FLL) — one step covering the
    // src/lib barrel plus the named-module-consuming lib modules (logging,
    // call_deadline), each compiled in its production module shape.
    // `list-tests`: one list-only compilation per test binary, printing the tests the
    // compiler actually registered. Created before the steps that attach lanes to it.
    const list_step = b.step(buildpkg.test_list.STEP_NAME, buildpkg.test_list.STEP_DESC);

    buildpkg.lib_tests.addTestStep(b, target, optimize, test_filters, deps, list_step);

    // `test-s3`: compile r2.zig against z3 standalone (build-wiring gate).
    buildpkg.s3.addTestStep(b, target, optimize, test_filters, list_step);

    // ── Run step ─────────────────────────────────────────────────────────────
    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    b.step("run", "Run agentsfleetd").dependOn(&run_cmd.step);

    // ── error-codes.mdx generator ─────────────────────────────────────────────
    // Mechanical, registry-only: no external deps beyond error_registry.zig +
    // globalIo(), so it's rooted directly in errors/ (sibling relative import)
    // rather than needing the bench bridge module's cross-boundary shape.
    const gen_error_codes = b.addExecutable(.{
        .name = S_GEN_ERROR_CODES,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/agentsfleetd/errors/gen_error_codes.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = S_COMMON, .module = common_mod }},
        }),
    });
    const run_gen_error_codes = b.addRunArtifact(gen_error_codes);
    b.step(S_GEN_ERROR_CODES, "Render error-codes.mdx from the registry to stdout").dependOn(&run_gen_error_codes.step);

    // ── Test step ─────────────────────────────────────────────────────────────
    const tests = b.addTest(.{
        .name = S_AGENTSFLEETD_TESTS,
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_AGENTSFLEETD_TESTS_ROOT),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = S_HTTPZ, .module = httpz_mod },
                .{ .name = S_CACHE, .module = cache_mod },
                .{ .name = S_PG, .module = pg_mod },
                .{ .name = S_POSTHOG, .module = posthog_mod },
                .{ .name = S_SCHEMA, .module = schema_mod },
                .{ .name = S_BUILD_OPTIONS, .module = build_options_mod },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_CALL_DEADLINE, .module = call_deadline_mod },
                .{ .name = S_YAML, .module = yaml_mod },
                .{ .name = S_S3, .module = s3_mod },
            },
        }),
        .filters = test_filters,
    });
    buildpkg.fixtures.addDaemon(b, tests.root_module);
    const run_tests = b.addRunArtifact(tests);
    b.step("test", "Run unit tests").dependOn(&run_tests.step);
    buildpkg.test_list.addLane(b, list_step, S_AGENTSFLEETD_TESTS, tests.root_module, S_AGENTSFLEETD_TESTS_ROOT_DIR);

    // `test-auth`: the src/agentsfleetd/auth/** portability gate (src/build/auth_tests.zig).
    buildpkg.auth_tests.addTestStep(b, target, optimize, test_filters, &.{
        .{ .name = S_HTTPZ, .module = httpz_mod },
        .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
        .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
        .{ .name = S_LOG, .module = log_mod },
        .{ .name = S_CONTRACT, .module = contract_mod },
        .{ .name = S_COMMON, .module = common_mod },
    }, list_step);

    if (with_bench_tools) {
        // ── zBench dependency ────────────────────────────────────────────────
        const zbench_dep = b.dependency(S_ZBENCH, .{
            .target = target,
            .optimize = optimize,
        });
        const zbench_mod = zbench_dep.module(S_ZBENCH);

        // ── bench bridge module ──────────────────────────────────────────────
        // Re-exports `src/` internals so bench exes under `tests/bench/` can
        // reach them. Rooted at `src/bench_exports.zig` (inside src/) so the
        // module-root walk stays legal under Zig 0.15.2's strict boundaries.
        const bench_app_mod = b.createModule(.{
            .root_source_file = b.path("src/agentsfleetd/bench_exports.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_HTTPZ, .module = httpz_mod },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
                // The credential broker's token store — benched via bench_exports.
                .{ .name = S_CACHE, .module = cache_mod },
            },
        });

        // ── Tier-1 micro-benchmark runner (zBench-backed) ────────────────────
        // HTTP loadgen is handled by `hey` in make/bench.mk.
        const bench_micro = b.addExecutable(.{
            .name = "bench-micro",
            .root_module = b.createModule(.{
                .root_source_file = b.path("tests/bench/micro.zig"),
                .target = target,
                .optimize = optimize,
                .imports = &.{
                    .{ .name = S_ZBENCH, .module = zbench_mod },
                    .{ .name = "bench_app", .module = bench_app_mod },
                },
            }),
        });

        const run_bench_micro = b.addRunArtifact(bench_micro);
        if (b.args) |args| run_bench_micro.addArgs(args);
        b.step("bench-micro", "Run Tier-1 zbench micro-benchmarks").dependOn(&run_bench_micro.step);

        // ── Redis XADD concurrency bench ─────────────────────────────────────
        // 8 producer threads × 1000 XADDs against a live Redis. Skip-by-default
        // unless BENCH_REDIS=1 — see tests/bench/redis_xadd_concurrency.zig.
        const bench_redis = b.addExecutable(.{
            .name = "bench-redis",
            .root_module = b.createModule(.{
                .root_source_file = b.path("tests/bench/redis_xadd_concurrency.zig"),
                .target = target,
                .optimize = optimize,
                .imports = &.{
                    .{ .name = "bench_app", .module = bench_app_mod },
                },
            }),
        });

        const run_bench_redis = b.addRunArtifact(bench_redis);
        if (b.args) |args| run_bench_redis.addArgs(args);
        b.step("bench-redis", "Run Redis XADD concurrency bench (BENCH_REDIS=1)").dependOn(&run_bench_redis.step);
    }

    // Installable backend test binary for coverage tooling (kcov/codecov).
    const install_tests = b.addInstallArtifact(tests, .{
        .dest_sub_path = S_AGENTSFLEETD_TESTS,
    });
    b.step("test-bin", "Build/install backend test binary for coverage").dependOn(&install_tests.step);
}
