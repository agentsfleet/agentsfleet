//! Facade: re-exports all public workspace handlers from focused sub-modules.
//! External importers (e.g. src/http/handler.zig) see an unchanged surface.

const std = @import("std");
const httpz = @import("httpz");
// M10_001: policy import removed — recordPolicyEvent queries dropped tables.
const workspace_billing = @import("../../state/workspace_billing.zig");
const workspace_credit = @import("../../state/workspace_credit.zig");
const obs_log = @import("../../observability/logging.zig");
const error_codes = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const common = @import("common.zig");

const wb = @import("workspaces_billing.zig");
const wbs = @import("workspaces_billing_summary.zig");
const wl = @import("workspaces_lifecycle.zig");
const wo = @import("workspaces_ops.zig");

pub const handleUpgradeWorkspaceToScale = wb.handleUpgradeWorkspaceToScale;
pub const handleSetWorkspaceScoringConfig = wb.handleSetWorkspaceScoringConfig;
pub const handleApplyWorkspaceBillingEvent = wb.handleApplyWorkspaceBillingEvent;
pub const handleGetWorkspaceBillingSummary = wbs.handleGetWorkspaceBillingSummary;
pub const handleCreateWorkspace = wl.handleCreateWorkspace;
pub const handlePauseWorkspace = wo.handlePauseWorkspace;
pub const handleSyncSpecs = wo.handleSyncSpecs;

test {
    _ = @import("workspaces_billing.zig");
    _ = @import("workspaces_lifecycle.zig");
}
