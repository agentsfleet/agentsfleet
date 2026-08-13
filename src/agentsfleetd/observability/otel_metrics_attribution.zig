//! Bounded provider and model attribution for evented metric samples.

const health = @import("metrics_otel.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const dims = @import("otel_metrics_dims.zig");
const payload = @import("otel_metrics_payload.zig");
const semconv = @import("semconv.zig");
const Mode = @import("../state/tenant_provider.zig").Mode;

pub const Attribution = struct {
    posture: Mode,
    provider: []const u8,
    model: []const u8,
};

/// Attach provider and model labels when each has a bounded wire identity.
/// An unrepresentable value is omitted and counted, never truncated.
pub fn appendProviderAndModel(sample: *payload.Sample, attr: Attribution) void {
    var keyed = attr.provider;
    if (semconv.providerOrdinal(attr.provider)) |ordinal| {
        _ = payload.addLabelAtIndex(sample, semconv.ATTR_PROVIDER_NAME, dims.providerValueIndex(ordinal));
        keyed = semconv.WELL_KNOWN_PROVIDERS[ordinal];
    } else {
        health.recordAttributeOmission(.provider_name, .unmapped_provider);
    }
    if (attr.model.len == 0) return;
    if (!payload.valueFits(attr.model)) {
        health.recordAttributeOmission(.request_model, .value_too_long);
        return;
    }
    if (!cardinality.admitModel(keyed, attr.model)) {
        health.recordAttributeOmission(.request_model, .budget_exhausted);
        return;
    }
    _ = payload.setDynamicLabel(sample, semconv.ATTR_REQUEST_MODEL, attr.model);
}
