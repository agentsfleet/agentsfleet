const std = @import("std");
const common = @import("common");
const balance_policy = @import("../config/balance_policy.zig");
const clerk_fetch_worker = @import("../auth/clerk_fetch_worker.zig");
const http_server = @import("../http/server.zig");
const http_handler = @import("../http/handler.zig");
const session_store_redis = @import("../session/session_store_redis.zig");
const audit_events = @import("../auth/audit_events.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const api_key_lookup = @import("api_key_lookup.zig");
const serve_runner_lookup = @import("serve_runner_lookup.zig");
const metrics = @import("../observability/metrics.zig");
const logging = @import("log");
const telemetry_mod = @import("../observability/telemetry.zig");
const preflight = @import("preflight.zig");
const error_codes = @import("../errors/error_registry.zig");
const serve_shutdown = @import("serve_shutdown.zig");
const serve_background = @import("serve_background.zig");
const pg = @import("pg");
const serve_r2 = @import("serve_r2.zig");
const serve_caches = @import("serve_caches.zig");
const serve_secrets = @import("serve_secrets.zig");
const serve_webhook_lookup = @import("serve_webhook_lookup.zig");
const subscription_hub = @import("../events/subscription_hub.zig");
const fleet_set_cache = @import("../events/fleet_set_cache.zig");
const stream_registry = @import("../http/stream_registry.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");
const serve_qstash = @import("serve_qstash.zig");
const serve_deadline = @import("serve_deadline.zig");
const serve_boot = @import("serve_boot.zig");

const log = logging.scoped(.agentsfleetd);

const EnvMap = common.env.Map;

const webhook_sig = auth_mw.webhook_sig_mod;
const svix_signature = auth_mw.svix_signature_mod;

pub fn run(io: std.Io, env_map: *const EnvMap, argv: []const [:0]const u8, alloc: std.mem.Allocator) !void {
    var otel_exporters = preflight.initOtelExporters(io, env_map, alloc);
    defer otel_exporters.deinit(alloc);
    // Bounded wait for in-flight Clerk metadata workers; stragglers own only
    // self-lifetime memory, so timing out cannot free state under them.
    defer clerk_fetch_worker.drainForShutdown();
    log.info("startup.serve_start", .{});

    const serve_port_override = serve_boot.parseArgsOrExit(argv);
    serve_boot.enforceEnvOrExit(env_map, alloc);
    var serve_cfg = serve_boot.loadServeConfigOrExit(env_map, alloc);
    defer serve_cfg.deinit();
    if (serve_port_override) |override| {
        serve_cfg.port = override;
    }
    serve_boot.setKekOrExit(serve_cfg.encryption_master_key);

    // The ONE deadline scheduler this process owns. Declared before every
    // network owner so its defer unwinds LAST: the HTTP server, background
    // workers, and the subscription hub are all joined before the registration
    // storage their interrupt targets point into is freed.
    var deadlines: serve_deadline.Owned = .{};
    const deadline_scheduler = deadlines.start(alloc);
    defer deadlines.deinit();

    const api_pool = preflight.connectDbPool(io, env_map, alloc, .api) catch std.process.exit(1);
    defer api_pool.deinit();

    var api_queue = serve_boot.connectRedisOrExit(io, env_map, alloc);
    defer api_queue.deinit();
    metrics.registerRedisPool(&api_queue.pool);
    // Defer order: clear FIRST at scope exit so a mid-shutdown /metrics
    // scrape can't dereference a deinit'd Pool.
    defer metrics.clearRegisteredRedisPool();

    const migrate_on_start = preflight.parseMigrateOnStart(env_map, alloc) catch std.process.exit(1);
    preflight.checkMigrations(io, env_map, alloc, api_pool, migrate_on_start) catch std.process.exit(1);

    // No rate-cache warm at boot. Rates load on first use and are invalidated by
    // the catalogue generation stored with them, so a bulk preload would be a
    // second way to fill one cache — and the two would drift. It also removes a
    // startup dependency: the daemon no longer refuses to boot because the
    // catalogue was briefly unreadable.
    defer model_rate_cache.deinit();

    var qstash_credentials = serve_qstash.load(alloc, api_pool, serve_cfg.platform_admin_workspace_id);
    defer if (qstash_credentials) |*credentials| credentials.deinit(alloc);

    var sessions = session_store_redis.SessionStore.init(
        alloc,
        &api_queue,
        serve_cfg.auth_session_code_pepper,
        serve_cfg.audit_log_pepper,
    );

    // Owner of live SSE streams + the shared pub/sub connection they fan out
    // from (borrows the queue pool's resolved config — torn down before it).
    var streams = stream_registry.init(alloc, io);
    var hub = subscription_hub.init(alloc, io);
    var fleet_sets = fleet_set_cache.init(alloc, io);
    defer serve_shutdown.deinitStreaming(&hub, &streams, &fleet_sets);
    hub.start(api_queue.pool.cfg, deadline_scheduler) catch |err| {
        log.err("startup.subscription_hub_failed", .{
            .error_code = error_codes.ERR_STARTUP_REDIS_CONNECT,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };
    log.info("startup.subscription_hub_ok", .{});

    // Webhook/backend secrets resolved ONCE at boot; borrowed by handlers + webhook
    // middleware for the process lifetime (null = unset → fail closed).
    var secrets = try serve_secrets.resolve(env_map, alloc);
    defer secrets.deinit();

    var r2_store = try serve_r2.resolve(env_map, alloc, io);
    defer if (r2_store) |*c| c.deinit();

    // Detached install-step workers borrow the pool + queue; teardown waits
    // for the in-flight ones here BEFORE the pool/queue defers (declared
    // above, so they unwind after this) free what the workers use.
    var install_wg: common.WaitGroup = .{};
    defer serve_shutdown.awaitInstallWorkers(&install_wg);

    defer serve_caches.deinit();
    var ctx = http_handler.Context{
        .model_library_cache = serve_caches.init(alloc),
        .pool = api_pool,
        .queue = &api_queue,
        .install_wg = &install_wg,
        .alloc = alloc,
        .io = io,
        .deadline_scheduler = deadline_scheduler,
        .clerk_webhook_secret = secrets.clerk_webhook_secret,
        .approval_signing_secret = secrets.approval_signing_secret,
        .clerk_secret_key = secrets.clerk_secret_key,
        .oidc = null,
        .r2 = if (r2_store) |*c| c else null,
        .auth_sessions = &sessions,
        .audit_ctx = audit_events.AuditCtx.init(serve_cfg.audit_log_pepper),
        .app_url = serve_cfg.app_url,
        .api_url = serve_cfg.api_url,
        .platform_admin_workspace_id = serve_cfg.platform_admin_workspace_id,
        .qstash_credentials = if (qstash_credentials) |*credentials| credentials else null,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = serve_cfg.api_max_in_flight_requests,
        .sse_max_streams = serve_cfg.sse_max_streams,
        .hub = &hub,
        .stream_registry = &streams,
        .fleet_sets = &fleet_sets,
        .ready_max_queue_depth = serve_cfg.ready_max_queue_depth,
        .ready_max_queue_age_ms = serve_cfg.ready_max_queue_age_ms,
        .balance_policy = balance_policy.resolveFromEnv(env_map, alloc),
        // SAFETY: written by surrounding init logic before any read of this storage.
        .telemetry = undefined,
    };
    defer ctx.deinitSlackSigningSecretCache();
    metrics.setApiInFlightRequests(0);
    metrics.setSseInFlightStreams(0);

    var tel = preflight.initTelemetry(env_map, alloc);
    defer tel.deinit(alloc);
    ctx.telemetry = tel.ptr();

    var oidc = try serve_boot.initOidc(alloc, &serve_cfg);
    defer if (oidc) |*v| v.deinit();
    if (oidc) |*v| {
        ctx.oidc = v;
        log.info("startup.oidc_init_ok", .{});
    }
    var cred_broker = preflight.installCredentialBroker(alloc, io, deadline_scheduler, api_pool, serve_cfg.platform_admin_workspace_id, &ctx.broker, &ctx.github_app_slug); // M102 §3 + §5 slug
    defer cred_broker.deinit();

    // Build the middleware registry at boot.
    // The webhook signing secret is the boot-resolved `approval_signing_secret_owned`
    // (above) — each request borrows it, paying no getenv. Missing → empty slice →
    // the middleware rejects every request on that route (fail-closed).
    //
    // LIFETIME: `registry` is a stack-allocated var in run(). It must stay
    // alive for the duration of the server. `initChains()` captures pointers
    // into registry fields; do NOT call initChains() before all fields are set,
    // and do NOT move/copy registry after calling initChains().
    const approval_signing_secret: []const u8 = if (secrets.approval_signing_secret) |s| s else "";

    var api_key_lookup_ctx = api_key_lookup.Ctx{ .pool = ctx.pool };
    var runner_lookup_ctx = serve_runner_lookup.Ctx{ .pool = ctx.pool };

    var registry = serve_boot.buildRegistry(ctx.oidc, &api_key_lookup_ctx, &runner_lookup_ctx, approval_signing_secret);
    // Construct the generic WebhookSig with concrete *pg.Pool type.
    // Must be declared before initChains() so the pointer is stable, but
    // the chain is set via setWebhookSig() after initChains().
    var webhook_sig_mw = webhook_sig.WebhookSig(*pg.Pool){
        .lookup_ctx = api_pool,
        .lookup_fn = serve_webhook_lookup.lookup,
    };
    // Svix middleware for Clerk resolves whsec_<base64> via the workspace vault.
    var svix_mw = svix_signature.SvixSignature(*pg.Pool){
        .lookup_ctx = api_pool,
        .lookup_fn = serve_webhook_lookup.lookupSvix,
    };
    registry.initChains();
    registry.setWebhookSig(webhook_sig_mw.middleware());
    registry.setSvixSig(svix_mw.middleware());
    log.info("startup.middleware_registry_ok", .{});

    serve_shutdown.reset();
    preflight.installSignalHandlers(serve_shutdown.onSignal);

    var background = serve_background.Threads.init();
    try background.start(api_pool, &api_queue, alloc, deadline_scheduler);
    defer background.stop();

    log.info("http.server_starting", .{
        .port = serve_cfg.port,
        .api_threads = serve_cfg.api_http_threads,
        .api_workers = serve_cfg.api_http_workers,
        .api_max_clients = serve_cfg.api_max_clients,
        .api_max_in_flight = serve_cfg.api_max_in_flight_requests,
        .sse_max_streams = serve_cfg.sse_max_streams,
    });
    ctx.telemetry.capture(telemetry_mod.ServerStarted, .{ .port = serve_cfg.port });
    const srv = http_server.Server.init(io, &ctx, &registry, .{
        .port = serve_cfg.port,
        .threads = serve_cfg.api_http_threads,
        .workers = serve_cfg.api_http_workers,
        .max_clients = @intCast(serve_cfg.api_max_clients),
    }) catch |err| {
        log.err("http.server_init_failed", .{ .err = @errorName(err) });
        return err;
    };
    defer srv.deinit();
    serve_shutdown.publishServer(srv);
    defer serve_shutdown.clearServer();
    // First unwind step after listen returns: shutdown() every live stream's
    // client fd while srv.deinit() is still joining request threads; new
    // streams are rejected from here. Rest of the choreography: deinitStreaming.
    defer streams.drain();

    srv.listen() catch |err| {
        log.err("http.server_exit_failed", .{ .err = @errorName(err) });
    };

    background.stop();
}

// Arg-parsing tests live in serve_test.zig; the streaming teardown sequence
// lives in serve_shutdown.deinitStreaming with the rest of the choreography.
comptime {
    _ = @import("serve_test.zig");
}
