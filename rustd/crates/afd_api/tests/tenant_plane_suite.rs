//! Tenant, workspace, and fleet HTTP regression suite.

mod harness;

#[path = "integration_fleet_schedules.rs"]
mod integration_fleet_schedules;

#[path = "connector_callback_route.rs"]
mod connector_callback_route;
#[path = "integration_connector_callback.rs"]
mod integration_connector_callback;
#[path = "integration_connector_refresh.rs"]
mod integration_connector_refresh;
#[path = "integration_connector_status.rs"]
mod integration_connector_status;

#[path = "auth_sessions.rs"]
mod auth_sessions;
#[path = "fleet_grants.rs"]
mod fleet_grants;
#[path = "fleet_memories.rs"]
mod fleet_memories;
#[path = "fleet_memories_input.rs"]
mod fleet_memories_input;
#[path = "fleet_messages.rs"]
mod fleet_messages;
#[path = "fleet_messages_input.rs"]
mod fleet_messages_input;
#[path = "fleet_streams.rs"]
mod fleet_streams;
#[path = "integration_auth_sessions.rs"]
mod integration_auth_sessions;
#[path = "integration_fleet_lifecycle.rs"]
mod integration_fleet_lifecycle;
#[path = "integration_fleet_memories.rs"]
mod integration_fleet_memories;
#[path = "integration_fleet_streams.rs"]
mod integration_fleet_streams;
#[path = "integration_tenant.rs"]
mod integration_tenant;
#[path = "integration_tenant_cli.rs"]
mod integration_tenant_cli;
#[path = "integration_tenant_models.rs"]
mod integration_tenant_models;
#[path = "integration_tenant_money.rs"]
mod integration_tenant_money;
#[path = "integration_workspace_approvals.rs"]
mod integration_workspace_approvals;
#[path = "integration_workspace_approvals_listing.rs"]
mod integration_workspace_approvals_listing;
#[path = "integration_workspace_preferences.rs"]
mod integration_workspace_preferences;
#[path = "tenant_api_keys.rs"]
mod tenant_api_keys;
#[path = "tenant_billing.rs"]
mod tenant_billing;
#[path = "tenant_cli_credential.rs"]
mod tenant_cli_credential;
#[path = "tenant_model_entry_input.rs"]
mod tenant_model_entry_input;
#[path = "tenant_model_entry_route.rs"]
mod tenant_model_entry_route;
#[path = "tenant_models.rs"]
mod tenant_models;
#[path = "tenant_provider_route.rs"]
mod tenant_provider_route;
#[path = "tenant_shape_parity.rs"]
mod tenant_shape_parity;
#[path = "tenant_workspaces.rs"]
mod tenant_workspaces;
#[path = "workspace_approvals.rs"]
mod workspace_approvals;
#[path = "workspace_events.rs"]
mod workspace_events;
#[path = "workspace_events_input.rs"]
mod workspace_events_input;
#[path = "workspace_fleet_libraries.rs"]
mod workspace_fleet_libraries;
#[path = "workspace_fleets.rs"]
mod workspace_fleets;
#[path = "workspace_fleets_input.rs"]
mod workspace_fleets_input;
#[path = "workspace_preferences.rs"]
mod workspace_preferences;
#[path = "workspace_secrets.rs"]
mod workspace_secrets;
