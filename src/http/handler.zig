//! HTTP request handlers for all control-plane endpoints.
//! Uses httpz request/response API. All responses follow the M1_002 error contract.

const std = @import("std");
const common = @import("handlers/common.zig");
const workspace_handlers = @import("handlers/workspaces.zig");
const skill_secret_handlers = @import("handlers/skill_secrets.zig");
const health_handlers = @import("handlers/health.zig");
const skill_secrets_http = @import("handlers/skill_secrets_http.zig");
const auth_sessions_http = @import("handlers/auth_sessions_http.zig");
const github_callback = @import("handlers/github_callback.zig");
const admin_platform_keys_http = @import("handlers/admin_platform_keys_http.zig");
const workspace_credentials_http = @import("handlers/workspace_credentials_http.zig");
const agent_relay_http = @import("handlers/agent_relay.zig");
const webhooks_http = @import("handlers/webhooks.zig");
const approval_http = @import("handlers/approval_http.zig");
const zombie_api_http = @import("handlers/zombie_api.zig");
const zombie_activity_api_http = @import("handlers/zombie_activity_api.zig");
const zombie_telemetry_http = @import("handlers/zombie_telemetry.zig");
const slack_oauth_http = @import("handlers/slack_oauth.zig");
const slack_events_http = @import("handlers/slack_events.zig");
const slack_interactions_http = @import("handlers/slack_interactions.zig");

pub const Context = common.Context;
pub const SkillSecretRoute = skill_secret_handlers.Route;

pub const handleCreateWorkspace = workspace_handlers.handleCreateWorkspace;
pub const handlePauseWorkspace = workspace_handlers.handlePauseWorkspace;
pub const handleSyncSpecs = workspace_handlers.handleSyncSpecs;
pub const handleUpgradeWorkspaceToScale = workspace_handlers.handleUpgradeWorkspaceToScale;
pub const handleApplyWorkspaceBillingEvent = workspace_handlers.handleApplyWorkspaceBillingEvent;
pub const handleGetWorkspaceBillingSummary = workspace_handlers.handleGetWorkspaceBillingSummary;
pub const handleSetWorkspaceScoringConfig = workspace_handlers.handleSetWorkspaceScoringConfig;
pub const handleHealthz = health_handlers.handleHealthz;
pub const handleReadyz = health_handlers.handleReadyz;
pub const handleMetrics = health_handlers.handleMetrics;

pub const handlePutWorkspaceSkillSecret = skill_secrets_http.handlePutWorkspaceSkillSecret;
pub const handleDeleteWorkspaceSkillSecret = skill_secrets_http.handleDeleteWorkspaceSkillSecret;

pub const handleCreateAuthSession = auth_sessions_http.handleCreateAuthSession;
pub const handlePollAuthSession = auth_sessions_http.handlePollAuthSession;
pub const handleCompleteAuthSession = auth_sessions_http.handleCompleteAuthSession;

pub const handleGitHubCallback = github_callback.handleGitHubCallback;

pub const handlePutAdminPlatformKey = admin_platform_keys_http.handlePutAdminPlatformKey;
pub const handleDeleteAdminPlatformKey = admin_platform_keys_http.handleDeleteAdminPlatformKey;
pub const handleGetAdminPlatformKeys = admin_platform_keys_http.handleGetAdminPlatformKeys;

pub const handlePutWorkspaceLlmCredential = workspace_credentials_http.handlePutWorkspaceLlmCredential;
pub const handleDeleteWorkspaceLlmCredential = workspace_credentials_http.handleDeleteWorkspaceLlmCredential;
pub const handleGetWorkspaceLlmCredential = workspace_credentials_http.handleGetWorkspaceLlmCredential;

pub const handleSpecTemplate = agent_relay_http.handleSpecTemplate;
pub const handleSpecPreview = agent_relay_http.handleSpecPreview;

pub const handleReceiveWebhook = webhooks_http.handleReceiveWebhook;
pub const handleApprovalCallback = approval_http.handleApprovalCallback;

// M2_001: Zombie CRUD + activity + credentials
pub const handleCreateZombie = zombie_api_http.handleCreateZombie;
pub const handleListZombies = zombie_api_http.handleListZombies;
pub const handleDeleteZombie = zombie_api_http.handleDeleteZombie;
pub const handleListActivity = zombie_activity_api_http.handleListActivity;
pub const handleStoreCredential = zombie_activity_api_http.handleStoreCredential;
pub const handleListCredentials = zombie_activity_api_http.handleListCredentials;

// M18_001: zombie execution telemetry
pub const handleZombieTelemetry = zombie_telemetry_http.handleZombieTelemetry;
pub const handleInternalTelemetry = zombie_telemetry_http.handleInternalTelemetry;

// M8_001: Slack plugin acquisition
pub const handleSlackInstall = slack_oauth_http.handleInstall;
pub const handleSlackCallback = slack_oauth_http.handleCallback;
pub const handleSlackEvent = slack_events_http.handleSlackEvent;
pub const handleSlackInteraction = slack_interactions_http.handleInteraction;

pub fn parseSkillSecretRoute(path: []const u8) ?SkillSecretRoute {
    return skill_secret_handlers.parseRoute(path);
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = true,
        .queue_dependency_ok = false,
    }));
}

test "integration: ready decision fails closed when db is unhealthy" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = false,
        .queue_dependency_ok = true,
    }));
}

test "integration: ready decision passes when dependencies are healthy" {
    try std.testing.expect(health_handlers.readyDecision(.{
        .db_ok = true,
        .queue_dependency_ok = true,
    }));
}
