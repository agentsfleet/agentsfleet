//! Tenant, workspace, and fleet HTTP regression suite.

mod harness;

#[path = "auth_sessions.rs"]
mod auth_sessions;
#[path = "fleet_grants.rs"]
mod fleet_grants;
#[path = "fleet_lifecycle_live.rs"]
mod fleet_lifecycle_live;
#[path = "fleet_memories.rs"]
mod fleet_memories;
#[path = "fleet_memories_input.rs"]
mod fleet_memories_input;
#[path = "fleet_memories_live.rs"]
mod fleet_memories_live;
#[path = "fleet_messages.rs"]
mod fleet_messages;
#[path = "fleet_messages_input.rs"]
mod fleet_messages_input;
#[path = "fleet_streams.rs"]
mod fleet_streams;
#[path = "fleet_streams_live.rs"]
mod fleet_streams_live;
#[path = "tenant_api_keys.rs"]
mod tenant_api_keys;
#[path = "tenant_billing.rs"]
mod tenant_billing;
#[path = "tenant_cli_credential.rs"]
mod tenant_cli_credential;
#[path = "tenant_cli_live.rs"]
mod tenant_cli_live;
#[path = "tenant_live.rs"]
mod tenant_live;
#[path = "tenant_models.rs"]
mod tenant_models;
#[path = "tenant_money_live.rs"]
mod tenant_money_live;
#[path = "tenant_shape_parity.rs"]
mod tenant_shape_parity;
#[path = "tenant_workspaces.rs"]
mod tenant_workspaces;
#[path = "workspace_approvals.rs"]
mod workspace_approvals;
#[path = "workspace_approvals_live.rs"]
mod workspace_approvals_live;
#[path = "workspace_events.rs"]
mod workspace_events;
#[path = "workspace_events_input.rs"]
mod workspace_events_input;
#[path = "workspace_fleets.rs"]
mod workspace_fleets;
#[path = "workspace_fleets_input.rs"]
mod workspace_fleets_input;
#[path = "workspace_preferences.rs"]
mod workspace_preferences;
#[path = "workspace_preferences_live.rs"]
mod workspace_preferences_live;
#[path = "workspace_secrets.rs"]
mod workspace_secrets;
