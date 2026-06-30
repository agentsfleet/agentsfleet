pub const core_foundation_sql = @embedFile("001_core_foundation.sql");
pub const vault_sql = @embedFile("002_vault_schema.sql");
// Created before platform_llm_keys (006) so that table can carry an inline FK
// (provider, model) → core.model_caps(provider, model_id).
pub const model_caps_sql = @embedFile("005_model_caps.sql");
pub const platform_llm_keys_sql = @embedFile("006_platform_llm_keys.sql");
pub const core_fleets_sql = @embedFile("007_core_fleets.sql");
pub const core_fleet_sessions_sql = @embedFile("008_core_fleet_sessions.sql");
pub const core_fleet_approval_gates_sql = @embedFile("009_core_fleet_approval_gates.sql");
pub const core_integration_grants_sql = @embedFile("010_core_integration_grants.sql");
pub const core_fleet_keys_sql = @embedFile("011_core_fleet_keys.sql");
pub const memory_entries_sql = @embedFile("013_memory_entries.sql");
pub const fleet_execution_telemetry_sql = @embedFile("014_fleet_execution_telemetry.sql");
pub const api_keys_sql = @embedFile("015_api_keys.sql");
pub const core_users_sql = @embedFile("016_core_users.sql");
pub const tenant_billing_sql = @embedFile("017_tenant_billing.sql");
pub const fleet_events_sql = @embedFile("018_fleet_events.sql");
pub const tenant_providers_sql = @embedFile("020_tenant_providers.sql");
pub const fleet_runners_sql = @embedFile("021_fleet_runners.sql");
pub const fleet_runner_leases_sql = @embedFile("022_fleet_runner_leases.sql");
pub const fleet_runner_affinity_sql = @embedFile("023_fleet_runner_affinity.sql");
pub const fleet_metering_periods_sql = @embedFile("024_fleet_metering_periods.sql");
pub const fleet_runner_events_sql = @embedFile("025_fleet_runner_events.sql");
pub const account_purge_gate_bypass_sql = @embedFile("026_account_purge_gate_bypass.sql");
pub const core_fleet_bundles_sql = @embedFile("027_core_fleet_bundles.sql");
pub const fleet_bundle_templates_sql = @embedFile("028_fleet_bundle_templates.sql");
pub const tenant_fleet_bundle_templates_sql = @embedFile("029_core_tenant_fleet_bundle_templates.sql");
