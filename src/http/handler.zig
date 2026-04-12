//! HTTP request handlers for all control-plane endpoints.
//! Uses httpz request/response API. All responses follow the M1_002 error contract.

const std = @import("std");
const common = @import("handlers/common.zig");
const workspace_handlers = @import("handlers/workspaces.zig");
const agents_handlers = @import("handlers/agents.zig");
const skill_secret_handlers = @import("handlers/skill_secrets.zig");
const health_handlers = @import("handlers/health.zig");
const harness_http = @import("handlers/harness_http.zig");
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
const memory_http = @import("handlers/memory_http.zig");

pub const Context = common.Context;
pub const SkillSecretRoute = skill_secret_handlers.Route;

pub const handleCreateWorkspace = workspace_handlers.handleCreateWorkspace;
pub const handlePauseWorkspace = workspace_handlers.handlePauseWorkspace;
pub const handleSyncSpecs = workspace_handlers.handleSyncSpecs;
pub const handleUpgradeWorkspaceToScale = workspace_handlers.handleUpgradeWorkspaceToScale;
pub const handleApplyWorkspaceBillingEvent = workspace_handlers.handleApplyWorkspaceBillingEvent;
pub const handleGetWorkspaceBillingSummary = workspace_handlers.handleGetWorkspaceBillingSummary;
pub const handleSetWorkspaceScoringConfig = workspace_handlers.handleSetWorkspaceScoringConfig;
pub const handleGetAgent = agents_handlers.handleGetAgent;

pub const handleHealthz = health_handlers.handleHealthz;
pub const handleReadyz = health_handlers.handleReadyz;
pub const handleMetrics = health_handlers.handleMetrics;

pub const handlePutHarnessSource = harness_http.handlePutHarnessSource;
pub const handleCompileHarness = harness_http.handleCompileHarness;
pub const handleActivateHarness = harness_http.handleActivateHarness;
pub const handleGetHarnessActive = harness_http.handleGetHarnessActive;

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

// M14_001: External-agent memory API
pub const handleMemoryStore = memory_http.handleMemoryStore;
pub const handleMemoryRecall = memory_http.handleMemoryRecall;
pub const handleMemoryList = memory_http.handleMemoryList;
pub const handleMemoryForget = memory_http.handleMemoryForget;

// M2_001: Zombie CRUD + activity + credentials
pub const handleCreateZombie = zombie_api_http.handleCreateZombie;
pub const handleListZombies = zombie_api_http.handleListZombies;
pub const handleDeleteZombie = zombie_api_http.handleDeleteZombie;
pub const handleListActivity = zombie_activity_api_http.handleListActivity;
pub const handleStoreCredential = zombie_activity_api_http.handleStoreCredential;
pub const handleListCredentials = zombie_activity_api_http.handleListCredentials;

pub fn parseSkillSecretRoute(path: []const u8) ?SkillSecretRoute {
    return skill_secret_handlers.parseRoute(path);
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = true,
        .worker_ok = true,
        .queue_dependency_ok = false,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: ready decision fails during worker restart window" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = true,
        .worker_ok = false,
        .queue_dependency_ok = true,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: ready decision passes when dependencies and guardrails are healthy" {
    try std.testing.expect(health_handlers.readyDecision(.{
        .db_ok = true,
        .worker_ok = true,
        .queue_dependency_ok = true,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}
