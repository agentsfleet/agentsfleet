const std = @import("std");

pub const FREE_PLAN_SKU = "free_v1";
pub const SCALE_PLAN_SKU = "scale_v1";
pub const DEFAULT_GRACE_PERIOD_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub const PlanTier = enum {
    free,
    scale,

    pub fn label(self: PlanTier) []const u8 {
        return switch (self) {
            .free => "FREE",
            .scale => "SCALE",
        };
    }
};

pub const BillingStatus = enum {
    active,
    grace,
    downgraded,

    pub fn label(self: BillingStatus) []const u8 {
        return switch (self) {
            .active => "ACTIVE",
            .grace => "GRACE",
            .downgraded => "DOWNGRADED",
        };
    }
};

pub const PendingStatus = enum {
    activate_scale,
    payment_failed,
    downgrade_to_free,

    pub fn label(self: PendingStatus) []const u8 {
        return switch (self) {
            .activate_scale => "ACTIVATE_SCALE",
            .payment_failed => "PAYMENT_FAILED",
            .downgrade_to_free => "DOWNGRADE_TO_FREE",
        };
    }
};

pub const BillingLifecycleEvent = enum {
    payment_failed,
    downgrade_to_free,
};

pub const StateView = struct {
    plan_tier: PlanTier,
    billing_status: BillingStatus,
    plan_sku: []const u8,
    subscription_id: ?[]const u8,
    grace_expires_at: ?i64,
};

pub const UpgradeInput = struct {
    subscription_id: []const u8,
    actor: []const u8,
};

pub const BillingLifecycleEventInput = struct {
    event: BillingLifecycleEvent,
    reason: []const u8,
    actor: []const u8,
};
