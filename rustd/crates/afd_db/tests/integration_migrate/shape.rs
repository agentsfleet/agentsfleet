//! Dimension 2.1 — migrating an empty database builds exactly the tables and
//! columns of the signed-off layout, in the nine owned schemas and nowhere else.
//!
//! The expected layout is [`LAYOUT`], held here. That
//! `docs/architecture/postgres_schemas.md` lists the same tables and columns as
//! the registered migrations is the architecture-doc lint's row
//! `test_arch_schema_layout_matches_doc` (`make check-architecture-doc`), so the
//! doc, the slots and this list cannot drift apart without one of them failing.

use std::collections::BTreeMap;

use super::*;

/// The nine owned schemas. `audit` holds only sqlx's ledger.
const OWNED_SCHEMAS: [&str; 9] = [
    "identity",
    "authoring",
    "fleet",
    "runner",
    "connector",
    "model",
    "billing",
    "vault",
    "memory",
];

/// The schema the migrator creates for sqlx's ledger.
const LEDGER_SCHEMA: &str = "audit";
/// sqlx's ledger: the migrator's, not a slot's.
const LEDGER_TABLE: &str = "audit._sqlx_migrations";

/// How many tables the slots build.
const TABLE_COUNT: usize = 49;

/// Every table the slots build, one per line as `schema.table: column, ...`,
/// columns in any order. A line ending in `\` continues on the next.
const LAYOUT: &str = "\
identity.tenants: id, name, created_at, updated_at
identity.workspaces: id, tenant_id, name, created_at
identity.users: id, tenant_id, oidc_subject, email, display_name, created_at, updated_at
identity.memberships: id, tenant_id, user_id, role, created_at
identity.invites: id, tenant_id, email, role, status, invited_by_user_id, accepted_by_user_id, \
    expires_at, resolved_at, email_status, email_attempt_count, email_sent_at, created_at, \
    updated_at
identity.api_keys: id, tenant_id, name, description, key_hash, created_by_subject, revoked_at, \
    created_at, updated_at
identity.cli_credentials: id, user_id, tenant_id, machine_name, credential_hash, \
    credential_prefix, deployment, created_from_address, revoked_at, created_at, updated_at
identity.user_preferences: id, user_id, workspace_id, name, value, created_at, updated_at
authoring.drafts: id, workspace_id, tenant_id, user_id, title, status, tenant_model_id, \
    head_candidate_id, source_type, source_library_revision_id, source_fleet_revision_id, \
    instance_name, fleet_id, library_revision_id, last_message_sequence, \
    cancelled_through_sequence, resource_version, closed_at, expires_at, created_at, updated_at
authoring.messages: id, draft_id, sequence, operation_id, status, tenant_model_id, user_text, \
    answer_text, candidate_id, failure_reason, finished_at, created_at, updated_at
authoring.candidates: id, draft_id, revision, definition, canonical_digest, policy_semantics, \
    created_at
authoring.attempts: id, lease_id, runner_id, workspace_id, tenant_id, user_id, draft_id, \
    message_sequence, fencing_token, model_token_hash, model_snapshot, status, lease_expires_at, \
    expires_at, reserved_nanos, usage_unknown, call_count, output_token_count, \
    reported_runtime_ms, last_activity_sequence, failure_reason, outcome, created_at, updated_at
authoring.dispatches: id, attempt_id, sequence, request_digest, status, result, usage, \
    native_reply, private_expires_at, dispatch_expires_at, failure_cause, created_at, updated_at
authoring.activity_receipts: id, attempt_id, sequence, request_digest, candidate_id, created_at
authoring.answer_receipts: id, draft_id, attempt_id, report_id, answer_digest, expires_at, \
    created_at
authoring.replay_receipts: id, workspace_id, user_id, scope, operation_id, request_digest, \
    resource_id, created_at
authoring.library_entries: id, tenant_id, visibility, platform_slug, name, description, \
    credential_reasons, head_revision_id, resource_version, created_at, updated_at
authoring.library_revisions: id, library_entry_id, revision, definition, canonical_digest, \
    policy_semantics, created_at
fleet.fleets: id, workspace_id, tenant_id, name, status, head_revision_id, required_tags, \
    setup_stream_ready, initial_start, memory_reads_workspace, memory_publishes_workspace, \
    resource_version, created_at, updated_at
fleet.revisions: id, fleet_id, revision, definition, canonical_digest, policy_semantics, \
    runtime_model_ref, review_digest, review_context, source_candidate_id, created_at
fleet.schedules: id, fleet_id, source, provider_schedule_id, cron_expression, timezone, message, \
    once, fire_at, recipe, desired_status, sync_status, resource_version, sync_token, \
    sync_lease_expires_at, last_error, created_at, updated_at
fleet.sessions: fleet_id, context_json, created_at, updated_at
fleet.events: id, fleet_id, workspace_id, event_id, actor, event_type, status, request_json, \
    response_text, tool_calls, token_count, wall_ms, failure_reason, failure_detail, \
    resumes_event_id, created_at, updated_at
fleet.admissions: id, sequence, fleet_id, workspace_id, producer, producer_key, payload_digest, \
    actor, event_type, request_json, event_created_at, reply_provider, reply_address, \
    delivered_at, created_at, updated_at, lease_fence, \
    parked_at, settled_at
fleet.approval_gates: id, fleet_id, workspace_id, event_id, active_grant_id, action_id, \
    tool_name, action_name, gate_type, proposed_action, evidence, stated_binding, blast_radius, \
    spend_count, spend_ceiling, expires_at, status, resolved_at, resolved_by, resolved_by_name, \
    detail, created_at, updated_at
fleet.obligations: id, sequence, fleet_id, workspace_id, provider, destination, event_id, \
    answer, receipt, delivered_at, attempt_count, abandoned_at, abandon_reason, created_at, \
    updated_at
fleet.tool_calls: id, workspace_id, fleet_id, fleet_event_id, fencing_token, sequence, \
    arguments, truncated_arguments, output, output_line_count, truncated, byte_count, \
    created_at, updated_at
fleet.activity_counters: fleet_id, event_count, budget_used_nanos, created_at, updated_at
runner.runners: id, host_id, token_hash, sandbox_tier, status, labels, network_policy, \
    registry_allowlist, worker_count, sandbox_config, capability_report, degraded_reason, \
    last_seen_at, selftest_requested_at, selftest_completed_at, selftest_checks, \
    selftest_sandbox_tier, selftest_network_policy, lease_version, last_lease_type, created_at, \
    updated_at
runner.leases: id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, \
    event_created_at, posture, provider, model, last_metered_at, fencing_token, \
    lease_expires_at, status, posted_message_count, created_at, updated_at
runner.fleet_slots: fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at, \
    created_at, updated_at
runner.events: id, runner_id, event_type, metadata, last_seen_at, created_at
runner.lifetime_counters: runner_id, acquired_count, succeeded_count, failed_count, \
    expired_count, created_at, updated_at
connector.installs: id, provider, external_account_id, workspace_id, scopes, created_at, \
    updated_at
connector.channels: id, provider, external_account_id, external_channel_id, fleet_id, \
    binding_type, created_at, updated_at
connector.integration_grants: id, fleet_id, service, status, requested_reason, approved_at, \
    revoked_at, created_at, updated_at
model.platform_models: id, model_name, provider, display_name, context_cap_tokens, \
    context_source_url, context_verified_at, advanced, inference, created_at, updated_at
model.platform_model_prices: id, platform_model_id, effective_from, effective_until, \
    prompt_tokens_above, input_nanos_per_mtok, cached_input_nanos_per_mtok, \
    cache_write_nanos_per_mtok, cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok, \
    output_nanos_per_mtok, source_url, verified_at, conditions, created_at, updated_at
model.catalogue_version: id, resource_version, created_at, updated_at
model.platform_defaults: provider, source_workspace_id, active, platform_model_id, base_url, \
    created_at, updated_at
model.tenant_models: id, tenant_id, model_name, secret_ref, created_at, updated_at
model.tenant_selections: tenant_id, tenant_model_id, created_at, updated_at
billing.wallets: tenant_id, balance_nanos, exhausted_at, created_at, updated_at
billing.authoring_report_receipts: id, attempt_id, report_id, accounting_digest, \
    acknowledgement, created_at
billing.usage_lines: id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, \
    event_id, authoring_attempt_id, provider, model, posture, requested_at, input_token_count, \
    cached_input_token_count, cache_write_token_count, cache_write_5m_token_count, \
    cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id, \
    unpriced_token_count, charged_nanos, settled_at, created_at, updated_at
billing.holds: id, work_type, work_id, tenant_id, ceiling_nanos, captured_nanos, released_at, \
    created_at, updated_at
vault.secrets: id, workspace_id, key_name, kek_version, dek_ciphertext, dek_nonce, dek_tag, \
    secret_nonce, secret_ciphertext, secret_tag, meta_kind, meta_provider, meta_base_url, \
    meta_has_key, created_at, updated_at
memory.entries: id, fleet_id, workspace_id, key, content, category, workspace_visible, \
    created_at, updated_at
memory.context_checkpoints: id, draft_id, lease_id, fleet_id, branch_id, resource_version, \
    document, created_at, updated_at
";

/// Every table outside the system schemas, with its columns in any order.
const BUILT_COLUMNS: &str = "\
SELECT c.table_schema || '.' || c.table_name, c.column_name::text
  FROM information_schema.columns c
  JOIN information_schema.tables t
    ON t.table_schema = c.table_schema AND t.table_name = c.table_name
 WHERE t.table_type = 'BASE TABLE'
   AND c.table_schema NOT IN ('pg_catalog', 'information_schema')";

/// Every schema the database holds beyond the system ones and `public`.
const BUILT_SCHEMAS: &str = "\
SELECT nspname::text FROM pg_namespace
 WHERE nspname NOT LIKE 'pg\\_%' AND nspname NOT IN ('information_schema', 'public')";

/// Table name → column names, as a set per table.
type Layout = BTreeMap<String, BTreeSet<String>>;

/// [`LAYOUT`], parsed.
fn designed() -> Layout {
    LAYOUT
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(table, columns)| {
            let columns = columns.split(',').map(|c| c.trim().to_owned()).collect();
            (table.trim().to_owned(), columns)
        })
        .collect()
}

/// What the migrated database holds, ledger excluded.
async fn built(connection: &mut PgConnection) -> Layout {
    let rows: Vec<(String, String)> = sqlx::query_as(BUILT_COLUMNS)
        .fetch_all(connection)
        .await
        .unwrap();
    let mut layout = Layout::new();
    for (table, column) in rows {
        if table != LEDGER_TABLE {
            layout.entry(table).or_default().insert(column);
        }
    }
    layout
}

/// The schema half of a `schema.table` name.
fn schema_of(table: &str) -> &str {
    table.split('.').next().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_from_empty_builds_nine_owned_schemas() {
    let design = designed();
    assert_eq!(
        design.len(),
        TABLE_COUNT,
        "the layout lists {TABLE_COUNT} tables"
    );
    let owned: BTreeSet<&str> = OWNED_SCHEMAS.into_iter().collect();

    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();
    let built = built(&mut connection).await;
    let schemas: Vec<String> = sqlx::query_scalar(BUILT_SCHEMAS)
        .fetch_all(&mut *connection)
        .await
        .unwrap();

    let mut expected_schemas = owned.clone();
    expected_schemas.insert(LEDGER_SCHEMA);
    let built_schemas: BTreeSet<&str> = schemas.iter().map(String::as_str).collect();
    assert_eq!(
        built_schemas, expected_schemas,
        "the schemas the slots create"
    );
    let table_schemas: BTreeSet<&str> = built.keys().map(|t| schema_of(t)).collect();
    assert_eq!(
        table_schemas, owned,
        "every owned schema holds a table, and only they do"
    );

    let designed_tables: BTreeSet<&String> = design.keys().collect();
    let built_tables: BTreeSet<&String> = built.keys().collect();
    assert_eq!(built_tables, designed_tables, "the tables the slots build");
    for (table, columns) in &design {
        assert_eq!(built.get(table), Some(columns), "the columns of {table}");
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
