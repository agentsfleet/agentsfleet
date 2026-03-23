//! Worker loop — Thread 2.
//! Reads Redis stream queue messages for run claims (fail-closed when queue is unavailable).
//! Executes pipeline stages from a topology profile (default: Echo → Scout → Warden).
//! Commits stage artifacts to git branch. Runs sequentially per run.

const std = @import("std");
const pg = @import("pg");
const posthog = @import("posthog");
const agents = @import("agents.zig");
const github_auth = @import("../auth/github.zig");
const backoff = @import("../reliability/backoff.zig");
const topology = @import("topology.zig");
const worker_state_mod = @import("worker_state.zig");
const worker_allocator = @import("worker_allocator.zig");
const worker_claim = @import("worker_claim.zig");
const worker_runtime = @import("worker_runtime.zig");
const worker_rate_limiter = @import("worker_rate_limiter.zig");
const sandbox_runtime = @import("sandbox_runtime.zig");
const metrics = @import("../observability/metrics.zig");
const queue_consts = @import("../queue/constants.zig");
const queue_redis = @import("../queue/redis.zig");
const obs_log = @import("../observability/logging.zig");
const log = std.log.scoped(.worker);

// ── Worker configuration ──────────────────────────────────────────────────

pub const WorkerConfig = struct {
    pool: *pg.Pool,
    config_dir: []const u8,
    cache_root: []const u8,
    github_app_id: []const u8,
    github_app_private_key: []const u8,
    pipeline_profile_path: []const u8,
    skill_registry: ?*const agents.SkillRegistry = null,
    max_attempts: u32 = 3,
    run_timeout_ms: u64 = 300_000,
    poll_interval_ms: u64 = 2_000,
    sandbox: sandbox_runtime.Config = .{},
    rate_limit_capacity: u32 = 30,
    rate_limit_refill_per_sec: f64 = 5.0,
    posthog: ?*posthog.PostHogClient = null,
};

// ── Worker state shared between HTTP and worker threads ──────────────────

pub const WorkerState = worker_state_mod.WorkerState;

// ── Run context ───────────────────────────────────────────────────────────

const WorkerAllocator = worker_allocator.WorkerAllocator;
const TenantRateLimiter = worker_rate_limiter.TenantRateLimiter;

// ── Entry point ───────────────────────────────────────────────────────────

pub fn workerLoop(cfg: WorkerConfig, worker_state: *WorkerState) void {
    metrics.setWorkerInFlightRuns(worker_state.currentInFlightRuns());
    var gpa = WorkerAllocator{};
    defer {
        worker_state.running.store(false, .release);
        const inflight = worker_state.currentInFlightRuns();
        if (inflight != 0) {
            log.warn("worker.exiting_with_inflight in_flight_runs={d}", .{inflight});
        }
        _ = worker_allocator.finalizeWorkerAllocator(&gpa);
    }
    const alloc = gpa.allocator();

    log.info("worker.started poll_interval_ms={d}", .{cfg.poll_interval_ms});

    const prompts = agents.loadPrompts(alloc, cfg.config_dir) catch |err| {
        obs_log.logErr(.worker, err, "worker.prompts_load_fail config_dir={s}", .{cfg.config_dir});
        return;
    };
    defer {
        alloc.free(prompts.echo);
        alloc.free(prompts.scout);
        alloc.free(prompts.warden);
    }

    var profile = topology.defaultProfile(alloc) catch |err| {
        obs_log.logErr(.worker, err, "worker.profile_init_fail", .{});
        return;
    };
    defer profile.deinit();
    log.info("worker.profile_loaded agent_id={s} stages={d}", .{ profile.agent_id, profile.stages.len });

    var token_cache = github_auth.TokenCache.init(alloc, cfg.github_app_id, cfg.github_app_private_key);
    defer token_cache.deinit();
    var tenant_limiter = TenantRateLimiter.init(alloc, cfg.rate_limit_capacity, cfg.rate_limit_refill_per_sec);
    defer tenant_limiter.deinit();

    var queue_client = queue_redis.Client.connectFromEnv(alloc, .worker) catch |err| {
        obs_log.logErr(.worker, err, "worker.redis_unavailable error_code=UZ-INTERNAL-001", .{});
        return;
    };
    defer queue_client.deinit();

    queue_client.ensureConsumerGroup() catch |err| {
        obs_log.logErr(.worker, err, "worker.redis_group_fail error_code=UZ-INTERNAL-003", .{});
        return;
    };

    const consumer_id = queue_redis.makeConsumerId(alloc) catch "worker-local";
    defer if (!std.mem.eql(u8, consumer_id, "worker-local")) alloc.free(consumer_id);
    var last_reclaim_ms: i64 = std.time.milliTimestamp();

    var consecutive_errors: u32 = 0;
    while (worker_state.running.load(.acquire)) {
        var queued_message: ?queue_redis.QueueMessage = null;
        const now_ms = std.time.milliTimestamp();
        if (now_ms - last_reclaim_ms >= queue_consts.reclaim_interval_ms) {
            last_reclaim_ms = now_ms;
            queued_message = queue_client.xautoclaimOne(consumer_id) catch |err| {
                obs_log.logErr(.worker, err, "worker.xautoclaim_fail error_code=UZ-INTERNAL-003", .{});
                metrics.incWorkerErrors();
                consecutive_errors += 1;
                const max_delay_ms = std.math.mul(u64, cfg.poll_interval_ms, 8) catch cfg.poll_interval_ms;
                const delay_ms = backoff.expBackoffJitter(consecutive_errors - 1, cfg.poll_interval_ms, max_delay_ms);
                worker_runtime.sleepWhileRunning(&worker_state.running, delay_ms);
                continue;
            };
        }

        if (queued_message == null) {
            queued_message = queue_client.xreadgroupOne(consumer_id) catch |err| {
                obs_log.logErr(.worker, err, "worker.xreadgroup_fail error_code=UZ-INTERNAL-003", .{});
                metrics.incWorkerErrors();
                consecutive_errors += 1;
                const max_delay_ms = std.math.mul(u64, cfg.poll_interval_ms, 8) catch cfg.poll_interval_ms;
                const delay_ms = backoff.expBackoffJitter(consecutive_errors - 1, cfg.poll_interval_ms, max_delay_ms);
                worker_runtime.sleepWhileRunning(&worker_state.running, delay_ms);
                continue;
            };
        }

        defer if (queued_message) |*m| m.deinit(alloc);
        const queued = queued_message orelse {
            consecutive_errors = 0;
            continue;
        };

        worker_claim.processNextRun(
            alloc,
            .{
                .pool = cfg.pool,
                .execute = .{
                    .cache_root = cfg.cache_root,
                    .max_attempts = cfg.max_attempts,
                    .run_timeout_ms = cfg.run_timeout_ms,
                    .sandbox = cfg.sandbox,
                    .skill_registry = cfg.skill_registry,
                    .posthog = cfg.posthog,
                },
            },
            worker_state,
            &prompts,
            &profile,
            &token_cache,
            &tenant_limiter,
            queued.run_id,
        ) catch |err| {
            if (err != worker_runtime.WorkerError.ShutdownRequested) {
                obs_log.logErr(.worker, err, "worker.run_processing_error error_code=UZ-INTERNAL-003", .{});
                metrics.incWorkerErrors();
                consecutive_errors += 1;

                const max_delay_ms = std.math.mul(u64, cfg.poll_interval_ms, 8) catch cfg.poll_interval_ms;
                const delay_ms = backoff.expBackoffJitter(consecutive_errors - 1, cfg.poll_interval_ms, max_delay_ms);
                worker_runtime.sleepWhileRunning(&worker_state.running, delay_ms);
            }
            continue;
        };
        consecutive_errors = 0;

        queue_client.xack(queued.message_id) catch |err| {
            obs_log.logWarnErr(.worker, err, "worker.xack_fail message_id={s}", .{queued.message_id});
        };

        if (!worker_state.running.load(.acquire)) break;
    }

    log.info("worker.stopped", .{});
}

test {
    _ = @import("worker_agent_test.zig");
}
