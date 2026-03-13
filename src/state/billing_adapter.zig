const std = @import("std");

pub const Mode = enum {
    noop,
    manual,
    provider_stub,
};

pub const AdapterError = error{
    InvalidMode,
    MissingProviderApiKey,
    AdapterUnavailable,
    AdapterRejected,
};

pub const Charge = struct {
    idempotency_key: []const u8,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    billable_unit: []const u8,
    billable_quantity: u64,
};

pub const DeliveryResult = struct {
    reference: []const u8,
};

pub const Adapter = union(Mode) {
    noop: void,
    manual: void,
    provider_stub: struct {
        api_key: []u8,
    },

    pub fn deinit(self: *Adapter, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .provider_stub => |stub| alloc.free(stub.api_key),
            else => {},
        }
    }

    pub fn modeLabel(self: Adapter) []const u8 {
        return switch (self) {
            .noop => "noop",
            .manual => "manual",
            .provider_stub => "provider_stub",
        };
    }
};

pub fn adapterFromEnv(alloc: std.mem.Allocator) AdapterError!Adapter {
    const raw_mode = std.process.getEnvVarOwned(alloc, "BILLING_ADAPTER_MODE") catch return Adapter{ .noop = {} };
    defer alloc.free(raw_mode);
    const mode = parseMode(std.mem.trim(u8, raw_mode, " \t\r\n")) catch return AdapterError.InvalidMode;

    return switch (mode) {
        .noop => Adapter{ .noop = {} },
        .manual => Adapter{ .manual = {} },
        .provider_stub => blk: {
            const api_key = std.process.getEnvVarOwned(alloc, "BILLING_PROVIDER_API_KEY") catch return AdapterError.MissingProviderApiKey;
            if (std.mem.trim(u8, api_key, " \t\r\n").len == 0) {
                alloc.free(api_key);
                return AdapterError.MissingProviderApiKey;
            }
            break :blk Adapter{ .provider_stub = .{ .api_key = api_key } };
        },
    };
}

pub fn deliver(
    alloc: std.mem.Allocator,
    adapter: Adapter,
    charge: Charge,
) AdapterError!DeliveryResult {
    return switch (adapter) {
        .noop => .{ .reference = "noop" },
        .manual => .{ .reference = "manual" },
        .provider_stub => {
            if (envEnabled(alloc, "BILLING_PROVIDER_STUB_OUTAGE")) {
                return AdapterError.AdapterUnavailable;
            }
            if (charge.billable_quantity == 0) {
                return AdapterError.AdapterRejected;
            }
            return .{ .reference = "provider_stub" };
        },
    };
}

fn parseMode(raw: []const u8) AdapterError!Mode {
    if (std.mem.eql(u8, raw, "noop")) return .noop;
    if (std.mem.eql(u8, raw, "manual")) return .manual;
    if (std.mem.eql(u8, raw, "provider_stub")) return .provider_stub;
    return AdapterError.InvalidMode;
}

fn envEnabled(alloc: std.mem.Allocator, name: []const u8) bool {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return false;
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    return std.mem.eql(u8, trimmed, "1") or std.ascii.eqlIgnoreCase(trimmed, "true");
}

test "adapterFromEnv defaults to noop" {
    const alloc = std.testing.allocator;
    std.posix.unsetenv("BILLING_ADAPTER_MODE");

    var adapter = try adapterFromEnv(alloc);
    defer adapter.deinit(alloc);
    try std.testing.expectEqualStrings("noop", adapter.modeLabel());
}

test "adapterFromEnv requires provider api key for provider_stub mode" {
    const alloc = std.testing.allocator;
    try std.posix.setenv("BILLING_ADAPTER_MODE", "provider_stub", true);
    std.posix.unsetenv("BILLING_PROVIDER_API_KEY");
    defer std.posix.unsetenv("BILLING_ADAPTER_MODE");

    try std.testing.expectError(AdapterError.MissingProviderApiKey, adapterFromEnv(alloc));
}
