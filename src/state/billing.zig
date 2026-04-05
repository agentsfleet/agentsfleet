const runtime = @import("./billing_runtime.zig");

pub const BillableUnit = runtime.BillableUnit;
pub const FinalizeOutcome = runtime.FinalizeOutcome;
pub const UsageSnapshot = runtime.UsageSnapshot;

pub const recordRuntimeStageUsage = runtime.recordRuntimeStageUsage;
pub const finalizeRunForBilling = runtime.finalizeRunForBilling;
pub const aggregateStageAgentSeconds = runtime.aggregateStageAgentSeconds;
pub const aggregateBillableQuantityFromSnapshots = runtime.aggregateBillableQuantityFromSnapshots;

test {
    _ = @import("./billing_test.zig");
    _ = @import("./billing_gate_test.zig");
}
