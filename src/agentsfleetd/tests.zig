//! Aggregate test root for the agentsfleetd binary — the `test` / `test-integration`
//! build targets root here (not `main.zig`) so the production entry point stays
//! free of test wiring. Importing `main.zig` below pulls in the prod module
//! graph and main's own inline tests; the remaining lines force every other
//! prod module and `*_test.zig` into the test compilation so their `test`
//! blocks run. Mirrors `src/lib/tests.zig` and `src/agentsfleetd/auth/tests.zig`.

const logging = @import("log");

test {
    _ = @import("main.zig");
    _ = @import("db/pool.zig");
    _ = @import("db/pg_query.zig");
    _ = @import("db/sql_splitter.zig");
    _ = @import("db/sql_splitter_test.zig");
    _ = @import("db/index_usage_integration_test.zig");
    _ = @import("db/index_usage_fleet_integration_test.zig");
    _ = @import("http/handlers/fleets/secret_list_integration_test.zig");
    _ = @import("config/env_vars.zig");
    _ = @import("config/load.zig");
    _ = @import("config/balance_policy.zig");
    _ = @import("config/runtime.zig");
    _ = @import("fleet_runtime/config.zig");
    _ = @import("fleet_runtime/yaml_frontmatter.zig");
    _ = @import("http/route_matchers.zig");
    _ = @import("http/handlers/fleets/secret_metadata.zig");
    _ = @import("fleet_runtime/activity_publisher.zig");
    _ = @import("fleet_runtime/metering.zig");
    _ = @import("util/strings/string_builder.zig");
    // Runner control-plane verbs' per-event prep, lifted from the deleted worker.
    _ = @import("fleet/fleet_session.zig");
    _ = @import("fleet/event_rows.zig");
    _ = @import("fleet/budget.zig");
    _ = @import("fleet/budget_test.zig");
    _ = @import("cron/main.zig");
    _ = @import("fleet/service_activity.zig");
    _ = @import("fleet/approval_gate.zig");
    _ = @import("fleet_runtime/approval_gate_async.zig");
    _ = @import("fleet/context_resolve.zig");
    _ = @import("fleet/secrets_resolve.zig");
    _ = @import("fleet/secrets_resolve_test.zig");
    _ = @import("credentials/integration.zig");
    _ = @import("credentials/integration_ctx.zig");
    _ = @import("credentials/integration_github.zig");
    _ = @import("credentials/integration_oauth_refresh.zig");
    _ = @import("credentials/integration_oauth_refresh_test.zig");
    _ = @import("credentials/broker.zig");
    _ = @import("credentials/broker_test.zig");
    _ = @import("credentials/serve_broker.zig");
    _ = @import("credentials/serve_broker_test.zig");
    _ = @import("http/handlers/connectors/state.zig");
    _ = @import("http/handlers/connectors/oauth2.zig");
    _ = @import("http/handlers/connectors/oauth_status.zig");
    _ = @import("http/handlers/connectors/registry.zig");
    _ = @import("http/handlers/connectors/registry_integration_test.zig");
    _ = @import("http/handlers/connectors/slack/callback.zig");
    _ = @import("http/handlers/connectors/oauth_refresh.zig");
    _ = @import("http/handlers/connectors/zoho/callback.zig");
    _ = @import("http/handlers/connectors/jira/callback.zig");
    _ = @import("http/handlers/connectors/linear/callback.zig");
    _ = @import("http/handlers/connectors/github/callback.zig");
    _ = @import("http/handlers/connectors/github/callback_integration_test.zig");
    _ = @import("http/handlers/connectors/oauth_providers_integration_test.zig");
    _ = @import("http/handlers/connectors/slack/oauth_callback_integration_test.zig");
    _ = @import("http/handlers/connectors/slack/events_integration_test.zig");
    _ = @import("http/handlers/ingress/github_integration_test.zig");
    _ = @import("http/handlers/ingress/qstash_integration_test.zig");
    _ = @import("http/handlers/connectors/slack/outbound_integration_test.zig");
    _ = @import("http/handlers/connectors/slack/thread_refetch_integration_test.zig");
    _ = @import("http/handlers/connectors/slack/channel_memory_integration_test.zig");
    _ = @import("auth/crypto/rs256_sign.zig");
    _ = @import("fleet/schema_migration_test.zig");
    _ = @import("fleet/control_plane_integration_test.zig");
    _ = @import("fleet/control_plane_policy_integration_test.zig");
    _ = @import("fleet/control_plane_grant_integration_test.zig");
    _ = @import("fleet/event_lifecycle_integration_test.zig");
    _ = @import("fleet/budget_integration_test.zig");
    _ = @import("fleet/budget_gate_integration_test.zig");
    _ = @import("fleet/event_lifecycle_reclaim_integration_test.zig");
    _ = @import("fleet/renewal_integration_test.zig");
    _ = @import("fleet/service_renew_integration_test.zig");
    _ = @import("fleet/service_token_splits_wire_test.zig");
    _ = @import("fleet/liveness_sweeper_integration_test.zig");
    _ = @import("http/fleet_operator_integration_test.zig");
    _ = @import("http/stream_registry.zig");
    _ = @import("http/fleet_runner_events_integration_test.zig");
    _ = @import("http/runner_enrollment_integration_test.zig");
    _ = @import("http/secrets_json_metadata_integration_test.zig");
    _ = @import("http/tenant_provider_platform_default_available_integration_test.zig");
    _ = @import("http/tenant_model_entries_integration_test.zig");
    _ = @import("http/user_preferences_integration_test.zig");
    _ = @import("http/workspace_onboarding_integration_test.zig");
    _ = @import("hmac_sig");
    _ = @import("crypto/hmac_sig_test.zig");
    _ = @import("fleet_runtime/webhook_verify.zig");
    _ = @import("fleet_runtime/webhook_verify_test.zig");
    _ = @import("fleet_runtime/webhook/normalizer/github.zig");
    _ = @import("cli/commands.zig");
    _ = @import("auth/claims.zig");
    _ = @import("auth/jwks.zig");
    _ = @import("session/session_store_redis_proto_test.zig");
    _ = @import("session/session_store_redis_integration_test.zig");
    _ = @import("session/session_store_redis_ttl_integration_test.zig");
    _ = @import("events/bus.zig");
    _ = @import("events/subscription_hub.zig");
    _ = @import("events/activity_channel.zig");
    _ = @import("events/fleet_set_cache.zig");
    _ = @import("observability/trace.zig");
    _ = @import("observability/metrics_redis_pool.zig");
    _ = @import("observability/otlp/ring.zig");
    _ = @import("observability/otlp/config.zig");
    _ = @import("observability/otlp/Client.zig");
    _ = @import("observability/otlp/exporter.zig");
    _ = @import("observability/otel_logs.zig");
    _ = @import("observability/otel_traces.zig");
    _ = @import("observability/otel_metrics.zig");
    _ = @import("observability/otel_metrics_payload.zig");
    _ = @import("observability/otel_metrics_aggregate.zig");
    _ = @import("observability/otel_metrics_cardinality.zig");
    _ = logging.sinks;
    _ = @import("state/tenant_billing.zig");
    _ = @import("state/tenant_model_entries.zig");
    _ = @import("state/user_preferences.zig");
    _ = @import("state/workspace_onboarding.zig");
    _ = @import("state/model_rate_cache.zig");
    _ = @import("state/model_rate_cache_integration_test.zig");
    _ = @import("state/model_library_seed_integration_test.zig");
    _ = @import("state/account_teardown.zig");
    _ = @import("state/account_teardown_test.zig");
    _ = @import("state/heroku_names.zig");
    _ = @import("state/heroku_names_test.zig");
    _ = @import("state/signup_bootstrap.zig");
    _ = @import("state/signup_bootstrap_store.zig");
    _ = @import("state/signup_bootstrap_test.zig");
    _ = @import("state/vault.zig");
    _ = @import("state/vault_test.zig");
    _ = @import("secrets/crypto_store.zig");
    _ = @import("secrets/crypto_store_test.zig");
    _ = @import("secrets/secure_memory_test.zig");
    _ = @import("secrets/zeroizing_allocator_test.zig");
    _ = @import("http/handlers/handler_auth_primitives_test.zig");
    _ = @import("http/handlers/auth/sessions_log_redaction_test.zig");
    _ = @import("http/handlers/auth/session_helpers_error_leak_test.zig");
    _ = @import("http/handlers/error_response_test.zig");
    _ = @import("http/handlers/hx_test.zig");
    _ = @import("http/sensitive_request_test.zig");
    _ = @import("http/handlers/tenant_provider_dispatch_test.zig");
    _ = @import("http/handlers/memory/handler_test.zig");
    _ = @import("http/handlers/memory/shapes_test.zig");
    _ = @import("cmd/serve_test.zig");
    _ = @import("config/env_resolve_test.zig");
    _ = @import("queue/redis.zig");
    _ = @import("queue/redis_pool_test.zig");
    _ = @import("queue/redis_pool_concurrency_test.zig");
    _ = @import("queue/redis_connection_test.zig");
    _ = @import("queue/redis_errors_test.zig");
    _ = @import("queue/redis_subscriber_test.zig");
    // Persistent Fleet Memory — role isolation + selection policy + adapter write-path tests.
    _ = @import("memory/fleet_memory_role_test.zig");
    _ = @import("memory/fleet_memory_test.zig");
    _ = @import("memory/fleet_memory_integration_test.zig");
    // Fleet CRUD, activity, router
    _ = @import("http/handlers/fleets/api.zig");
    _ = @import("http/handlers/fleets/api_integration_test.zig");
    _ = @import("http/handlers/fleets/get_integration_test.zig");
    _ = @import("http/handlers/fleets/events_cost_integration_test.zig");
    _ = @import("http/handlers/fleets/list_aggregate_integration_test.zig");
    _ = @import("http/handlers/fleets/patch_if_match_integration_test.zig");
    _ = @import("http/handlers/memory/memory_forget_integration_test.zig");
    _ = @import("http/handlers/library/catalog_etag_integration_test.zig");
    _ = @import("http/handlers/fleets/create.zig");
    _ = @import("http/handlers/fleets/create_install_steps.zig");
    _ = @import("http/handlers/fleets/create_install_steps_lifecycle_test.zig");
    _ = @import("http/handlers/fleets/list.zig");
    _ = @import("http/handlers/fleets/patch.zig");
    _ = @import("http/handlers/fleets/patch_body_fields_integration_test.zig");
    _ = @import("http/handlers/fleets/patch_concurrent_integration_test.zig");
    _ = @import("http/handlers/fleets/cron_lifecycle_integration_test.zig");
    _ = @import("http/handlers/fleets/delete.zig");
    _ = @import("http/handlers/fleet_bundles/api_integration_test.zig");
    _ = @import("http/handlers/fleet_bundles/resolve.zig");
    // Two-tier template onboarding + gallery (M103)
    _ = @import("http/handlers/library/onboard.zig");
    _ = @import("http/handlers/library/gallery.zig");
    _ = @import("http/handlers/library/onboard_integration_test.zig");
    _ = @import("http/handlers/library/catalog.zig");
    _ = @import("http/handlers/library/entry_view.zig");
    _ = @import("http/handlers/library/catalog_integration_test.zig");
    _ = @import("http/handlers/library/catalog_patch_integration_test.zig");
    _ = @import("fleet_library/library_store.zig");
    _ = @import("fleet_library/importer.zig");
    _ = @import("fleet_library/github_source.zig");
    _ = @import("fleet_library/github_net.zig");
    // Fleet execution telemetry store (writers via metering, tenant-scoped read via /v1/tenants/me/billing/charges)
    _ = @import("state/fleet_telemetry_store.zig");
    _ = @import("http/handlers/workspaces/dashboard_integration_test.zig");
    _ = @import("http/handlers/workspaces/create_integration_test.zig");
    _ = @import("http/handlers/tenant_workspaces.zig");
    _ = @import("http/handlers/tenant_workspaces_integration_test.zig");
    _ = @import("http/router_test.zig");
    // Harness HTTP message-type unit tests (relocated from test_harness.zig)
    _ = @import("http/test_harness_test.zig");
    // Integration grant API
    _ = @import("http/handlers/integration_grants/handler.zig");
    _ = @import("http/handlers/api_keys/fleet.zig");
    _ = @import("http/handlers/api_keys/tenant.zig");
    _ = @import("http/handlers/api_keys/list.zig");
    _ = @import("http/handlers/api_keys/tenant_integration_test.zig");
    _ = @import("http/handlers/fleet/runners_list.zig");
    _ = @import("http/handlers/fleet/runners_list_test.zig");
    _ = @import("http/handlers/tenant_billing_integration_test.zig");
    _ = @import("http/handlers/model_library.zig");
    _ = @import("http/handlers/model_library_integration_test.zig");
    _ = @import("http/handlers/admin/model_library_admin.zig");
    _ = @import("http/handlers/admin/model_library_admin_integration_test.zig");
    _ = @import("http/handlers/admin/model_library_admin_delete_guard_test.zig");
    _ = @import("http/handlers/webhooks/grant_approval.zig");
    _ = @import("http/handlers/auth/identity_events_clerk_integration_test.zig");
    _ = @import("http/handlers/webhooks/github.zig");
    _ = @import("fleet_runtime/notifications/grant_notifier.zig");
    _ = @import("http/handlers/fleets/messages.zig");
    // Chat ingress — POST /v1/.../fleets/{id}/messages
    _ = @import("http/handlers/fleets/messages_integration_test.zig");
    _ = @import("http/handlers/schedules/api_integration_test.zig");
    _ = @import("http/handlers/memory/memories_integration_test.zig");
    _ = @import("http/handlers/runner/memory_fencing_test.zig");
    _ = @import("http/handlers/runner/credentials_mint_integration_test.zig");
    _ = @import("http/handlers/runner/memory_loop_integration_test.zig");
    _ = @import("http/handlers/runner/bundles.zig");
    _ = @import("http/handlers/fleets/events_integration_test.zig");
    // The failure cause is durable end to end: report write → column → envelope.
    _ = @import("http/handlers/fleets/events_failure_detail_integration_test.zig");
    _ = @import("http/handlers/approvals/inbox_integration_test.zig");
    _ = @import("http/handlers/fleets/sse_streaming_integration_test.zig");
    _ = @import("http/handlers/fleets/backpressure_integration_test.zig");
    // Cross-workspace IDOR regression tests (RULE WAUTH)
    _ = @import("http/handlers/cross_workspace_idor_test.zig");
    // RLS tenant-context resolution (use-after-free regression on the null-tenant lookup)
    _ = @import("http/handlers/tenant_context_integration_test.zig");
    // Applied-migration-version set (extracted from pool_migrations for FLL)
    _ = @import("db/migration_versions.zig");
    _ = @import("types/id_format.zig");
    _ = @import("types/id_format_test.zig");
    // billing/credit edge, idempotency + concurrency coverage
    _ = @import("state/tenant_billing_edge_test.zig");
    _ = @import("fleet_runtime/metering_edge_test.zig");
    _ = @import("fleet_runtime/metering_idempotent_test.zig");
    _ = @import("fleet_runtime/metering_concurrency_test.zig");
    // fleet lease/renewal concurrency + roundtrip integration coverage
    _ = @import("fleet/renewal_edge_test.zig");
    _ = @import("fleet/renewal_malformed_test.zig");
    _ = @import("fleet/renewal_metering_test.zig");
    _ = @import("fleet/concurrency_lease_test.zig");
    _ = @import("fleet/concurrency_renew_test.zig");
    _ = @import("fleet/integration_roundtrip_test.zig");
    _ = @import("fleet/integration_session_continuation_test.zig");
    _ = @import("fleet/placement_eligibility_test.zig");
    // Its `test { _ = importer; _ = store; }` façade never compiled, so it registered
    // nothing; `importer.zig` happens to be force-imported below, and `store.zig` has
    // no tests yet. Wiring the façade makes it do the job it was written for.
    _ = @import("fleet_library/mod.zig");
    // `cmd/*` reaches the test root only through main.zig's ordinary imports,
    // which register nothing — these lines are what make their blocks compile.
    _ = @import("cmd/common.zig");
    _ = @import("cmd/doctor.zig");
    _ = @import("cmd/doctor_args.zig");
    _ = @import("cmd/doctor_render.zig");
    _ = @import("cmd/preflight_test.zig");
    _ = @import("cmd/serve_shutdown.zig");
    _ = @import("cmd/serve_lifecycle_integration_test.zig");
}
