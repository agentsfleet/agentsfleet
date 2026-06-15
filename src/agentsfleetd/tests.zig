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
    _ = @import("config/env_vars.zig");
    _ = @import("config/load.zig");
    _ = @import("config/balance_policy.zig");
    _ = @import("config/runtime.zig");
    _ = @import("agent/config.zig");
    _ = @import("agent/yaml_frontmatter.zig");
    _ = @import("http/route_matchers.zig");
    _ = @import("agent/activity_publisher.zig");
    _ = @import("agent/metering.zig");
    _ = @import("util/strings/string_builder.zig");
    // Runner control-plane verbs' per-event prep, lifted from the deleted worker.
    _ = @import("fleet/agent_session.zig");
    _ = @import("fleet/event_rows.zig");
    _ = @import("fleet/service_activity.zig");
    _ = @import("fleet/approval_gate.zig");
    _ = @import("agent/approval_gate_async.zig");
    _ = @import("fleet/context_resolve.zig");
    _ = @import("fleet/secrets_resolve.zig");
    _ = @import("fleet/secrets_resolve_test.zig");
    _ = @import("fleet/schema_migration_test.zig");
    _ = @import("fleet/control_plane_integration_test.zig");
    _ = @import("fleet/event_lifecycle_integration_test.zig");
    _ = @import("fleet/renewal_integration_test.zig");
    _ = @import("fleet/service_renew_integration_test.zig");
    _ = @import("fleet/service_token_splits_wire_test.zig");
    _ = @import("fleet/liveness_sweeper_integration_test.zig");
    _ = @import("http/fleet_operator_integration_test.zig");
    _ = @import("http/stream_registry.zig");
    _ = @import("http/fleet_runner_events_integration_test.zig");
    _ = @import("http/runner_enrollment_integration_test.zig");
    _ = @import("hmac_sig");
    _ = @import("crypto/hmac_sig_test.zig");
    _ = @import("agent/webhook_verify.zig");
    _ = @import("agent/webhook_verify_test.zig");
    _ = @import("agent/webhook/normalizer/github.zig");
    _ = @import("cli/commands.zig");
    _ = @import("auth/claims.zig");
    _ = @import("auth/jwks.zig");
    _ = @import("session/session_store_redis_proto_test.zig");
    _ = @import("session/session_store_redis_integration_test.zig");
    _ = @import("session/session_store_redis_ttl_integration_test.zig");
    _ = @import("events/bus.zig");
    _ = @import("events/subscription_hub.zig");
    _ = @import("observability/trace.zig");
    _ = @import("observability/metrics_redis_pool.zig");
    _ = @import("observability/otel_logs.zig");
    _ = @import("observability/otel_traces.zig");
    _ = logging.sinks;
    _ = @import("state/tenant_billing.zig");
    _ = @import("state/account_teardown.zig");
    _ = @import("state/account_teardown_test.zig");
    _ = @import("state/heroku_names.zig");
    _ = @import("state/heroku_names_test.zig");
    _ = @import("state/signup_bootstrap.zig");
    _ = @import("state/signup_bootstrap_store.zig");
    _ = @import("state/signup_bootstrap_test.zig");
    _ = @import("state/vault.zig");
    _ = @import("state/vault_test.zig");
    _ = @import("http/handlers/handler_auth_primitives_test.zig");
    _ = @import("http/handlers/auth/sessions_log_redaction_test.zig");
    _ = @import("http/handlers/error_response_test.zig");
    _ = @import("http/handlers/hx_test.zig");
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
    // Persistent Agent Memory — role isolation + selection policy + adapter write-path tests.
    _ = @import("memory/agent_memory_role_test.zig");
    _ = @import("memory/agent_memory_test.zig");
    _ = @import("memory/agent_memory_integration_test.zig");
    // Agent CRUD, activity, router
    _ = @import("http/handlers/agents/api.zig");
    _ = @import("http/handlers/agents/api_integration_test.zig");
    _ = @import("http/handlers/agents/create.zig");
    _ = @import("http/handlers/agents/list.zig");
    _ = @import("http/handlers/agents/patch.zig");
    _ = @import("http/handlers/agents/patch_body_fields_integration_test.zig");
    _ = @import("http/handlers/agents/patch_concurrent_integration_test.zig");
    _ = @import("http/handlers/agents/delete.zig");
    // Agent execution telemetry store (writers via metering, tenant-scoped read via /v1/tenants/me/billing/charges)
    _ = @import("state/agent_telemetry_store.zig");
    _ = @import("http/handlers/workspaces/dashboard_integration_test.zig");
    _ = @import("http/handlers/workspaces/create_integration_test.zig");
    _ = @import("http/handlers/tenant_workspaces.zig");
    _ = @import("http/handlers/tenant_workspaces_integration_test.zig");
    _ = @import("http/router_test.zig");
    // Harness HTTP message-type unit tests (relocated from test_harness.zig)
    _ = @import("http/test_harness_test.zig");
    // Integration grant API
    _ = @import("http/handlers/integration_grants/handler.zig");
    _ = @import("http/handlers/api_keys/agent.zig");
    _ = @import("http/handlers/api_keys/tenant.zig");
    _ = @import("http/handlers/api_keys/list.zig");
    _ = @import("http/handlers/api_keys/tenant_integration_test.zig");
    _ = @import("http/handlers/fleet/runners_list.zig");
    _ = @import("http/handlers/fleet/runners_list_test.zig");
    _ = @import("http/handlers/tenant_billing_integration_test.zig");
    _ = @import("http/handlers/model_caps.zig");
    _ = @import("http/handlers/model_caps_integration_test.zig");
    _ = @import("http/handlers/webhooks/grant_approval.zig");
    _ = @import("http/handlers/auth/identity_events_clerk_integration_test.zig");
    _ = @import("http/handlers/webhooks/github.zig");
    _ = @import("agent/notifications/grant_notifier.zig");
    _ = @import("http/handlers/agents/messages.zig");
    // Chat ingress — POST /v1/.../agents/{id}/messages
    _ = @import("http/handlers/agents/messages_integration_test.zig");
    _ = @import("http/handlers/memory/memories_integration_test.zig");
    _ = @import("http/handlers/runner/memory_fencing_test.zig");
    _ = @import("http/handlers/runner/memory_loop_integration_test.zig");
    _ = @import("http/handlers/agents/events_integration_test.zig");
    _ = @import("http/handlers/approvals/inbox_integration_test.zig");
    _ = @import("http/handlers/agents/sse_streaming_integration_test.zig");
    _ = @import("http/handlers/agents/backpressure_integration_test.zig");
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
    _ = @import("agent/metering_edge_test.zig");
    _ = @import("agent/metering_idempotent_test.zig");
    _ = @import("agent/metering_concurrency_test.zig");
    // fleet lease/renewal concurrency + roundtrip integration coverage
    _ = @import("fleet/renewal_edge_test.zig");
    _ = @import("fleet/renewal_malformed_test.zig");
    _ = @import("fleet/renewal_metering_test.zig");
    _ = @import("fleet/concurrency_lease_test.zig");
    _ = @import("fleet/concurrency_renew_test.zig");
    _ = @import("fleet/integration_roundtrip_test.zig");
    _ = @import("fleet/integration_session_continuation_test.zig");
    _ = @import("fleet/placement_eligibility_test.zig");
}
