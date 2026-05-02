//! Tenant-scoped LLM provider state. Mode enum and platform-default
//! constants used by metering and billing. The resolver, CRUD entry
//! points, and HTTP handler arrive in the follow-up implementation
//! commits — this module currently ships only the surface that other
//! call sites need at compile time.

pub const Mode = enum {
    platform,
    byok,

    pub fn label(self: Mode) []const u8 {
        return switch (self) {
            .platform => "platform",
            .byok => "byok",
        };
    }
};

/// Platform-default model resolved when a tenant has no explicit
/// tenant_providers row OR has an explicit row with mode=platform.
pub const PLATFORM_DEFAULT_MODEL: []const u8 = "accounts/fireworks/models/kimi-k2.6";
