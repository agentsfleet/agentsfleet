//! Registry tests: the pinned names, units, and derived budget are asserted
//! against the sources they claim to follow, and the aliases the registry must
//! refuse are proven absent from every live descriptor.
//!
//! These are deliberately literal-heavy. The whole point of the registry is
//! that one file spells each wire string exactly once, so the test that guards
//! it must spell the expected value independently — a test that reads the
//! constant back would pass for any value.

const std = @import("std");
const semconv = @import("semconv.zig");
const payload = @import("otel_metrics_payload.zig");
const families = @import("otel_metrics_families.zig");
const aggregate = @import("otel_metrics_aggregate.zig");

const ALL_METRICS = [_]payload.MetricId{
    .invoke_agent_duration,
    .token_usage,
    .cache_read_token_usage,
    .credit_consumed,
    .samples_dropped,
};

test "test_semantic_registry_matches_pinned_sources" {
    // Only the core schema URL may ship: the pinned GenAI commit publishes none.
    try std.testing.expectEqualStrings("https://opentelemetry.io/schemas/1.43.0", semconv.CORE_SCHEMA_URL); // pin test: literal is the contract
    try std.testing.expectEqualStrings("agentsfleet", semconv.SERVICE_NAMESPACE); // pin test: literal is the contract

    // The one standard metric: `wall_ms` bounds exactly one agent invocation.
    try std.testing.expectEqualStrings("gen_ai.invoke_agent.duration", semconv.METRIC_INVOKE_AGENT_DURATION); // pin test: literal is the contract
    // Aggregate run facts stay product-namespaced — they are not client calls.
    try std.testing.expectEqualStrings("agentsfleet.invoke_agent.token.usage", semconv.METRIC_INVOKE_AGENT_TOKEN_USAGE); // pin test: literal is the contract
    try std.testing.expectEqualStrings("agentsfleet.invoke_agent.cache_read.token.usage", semconv.METRIC_INVOKE_AGENT_CACHE_READ); // pin test: literal is the contract
    try std.testing.expectEqualStrings("agentsfleet.billing.credit.consumed", semconv.METRIC_BILLING_CREDIT_CONSUMED); // pin test: literal is the contract
}

test "no live descriptor uses a rejected metric name" {
    for (ALL_METRICS) |id| {
        const name = families.metaFor(id).name;
        for (semconv.REJECTED_METRIC_NAMES) |rejected| {
            try std.testing.expect(!std.mem.eql(u8, name, rejected));
        }
    }
}

test "billing quantities never declare a time unit" {
    // The superseded series called nanocredits `ns`, which made a money figure
    // read as a duration in every unit-aware backend.
    const credit = families.metaFor(.credit_consumed);
    try std.testing.expectEqualStrings("{nanocredit}", credit.unit); // pin test: literal is the contract
    try std.testing.expect(!std.mem.eql(u8, credit.unit, "ns"));
    try std.testing.expect(!std.mem.eql(u8, credit.unit, semconv.UNIT_SECONDS));
}

test "no live metric name embeds its unit" {
    for (ALL_METRICS) |id| {
        const name = families.metaFor(id).name;
        try std.testing.expect(std.mem.indexOf(u8, name, "_ms") == null);
        try std.testing.expect(std.mem.indexOf(u8, name, "_nanos") == null);
        try std.testing.expect(std.mem.indexOf(u8, name, "_seconds") == null);
    }
}

test "duration declares seconds and buckets the pinned agent boundaries" {
    const duration = families.metaFor(.invoke_agent_duration);
    try std.testing.expectEqualStrings("s", duration.unit); // pin test: literal is the contract
    try std.testing.expectEqual(families.Scale.millis_to_seconds, duration.scale);
    // `gen_ai.invoke_agent.duration`'s OWN pinned boundaries, expressed in the
    // milliseconds the runner reports: 0.1s .. 409.6s. The client-call table
    // (0.01s .. 81.92s) belongs to `gen_ai.client.operation.duration` and would
    // saturate here — a real agent run outlives its top bucket.
    const expected = [_]u64{ 100, 200, 400, 800, 1600, 3200, 6400, 12800, 25600, 51200, 102400, 204800, 409600 }; // pin test: literal is the contract
    try std.testing.expectEqualSlices(u64, &expected, duration.bounds);
}

test "token usage declares the token annotation and never converts to seconds" {
    for ([_]payload.MetricId{ .token_usage, .cache_read_token_usage }) |id| {
        const meta = families.metaFor(id);
        try std.testing.expectEqualStrings("{token}", meta.unit); // pin test: literal is the contract
        try std.testing.expectEqual(families.Scale.none, meta.scale);
        try std.testing.expectEqualSlices(u64, &semconv.TOKEN_BUCKET_BOUNDS, meta.bounds);
    }
}

test "every histogram bucket table fits the payload bucket array" {
    for (ALL_METRICS) |id| {
        const meta = families.metaFor(id);
        if (meta.kind != .histogram) {
            try std.testing.expectEqual(@as(usize, 0), meta.bounds.len);
            continue;
        }
        try std.testing.expect(meta.bounds.len + 1 <= payload.N_BUCKETS);
    }
}

test "provider normalization admits only exact well-known names" {
    try std.testing.expectEqualStrings("anthropic", semconv.normalizeProvider("anthropic").?); // pin test: literal is the contract
    try std.testing.expectEqualStrings("openai", semconv.normalizeProvider("openai").?); // pin test: literal is the contract
    // No case folding, no prefix matching, no separator coercion: each of these
    // would publish a private spelling under a standard key.
    try std.testing.expect(semconv.normalizeProvider("Anthropic") == null);
    try std.testing.expect(semconv.normalizeProvider("anthropic-beta") == null);
    try std.testing.expect(semconv.normalizeProvider("aws_bedrock") == null);
    try std.testing.expect(semconv.normalizeProvider("") == null);
}

test "model attribution cap is derived and provably fits the series ceiling" {
    const cap = semconv.modelAttributionCap(aggregate.MAX_SERIES);
    try std.testing.expect(cap > 0);
    // The whole point of the derivation: a full budget plus the unattributed
    // shape plus the exporter self-signal still fits one flush window.
    const worst_case = cap * semconv.SERIES_PER_MODEL_PAIR + semconv.RESERVED_SERIES;
    try std.testing.expect(worst_case <= aggregate.MAX_SERIES);
    // And it is a real ceiling — one more pair would not fit.
    const overflow = (cap + 1) * semconv.SERIES_PER_MODEL_PAIR + semconv.RESERVED_SERIES;
    try std.testing.expect(overflow > aggregate.MAX_SERIES);
}

test "a ceiling below the reserved shape admits no model attribution" {
    try std.testing.expectEqual(@as(usize, 0), semconv.modelAttributionCap(semconv.RESERVED_SERIES));
    try std.testing.expectEqual(@as(usize, 0), semconv.modelAttributionCap(0));
}

test "forbidden metric attributes cover tenant identity and the private keys" {
    const must_be_forbidden = [_][]const u8{
        semconv.ATTR_WORKSPACE_ID,
        semconv.ATTR_TENANT_ID,
        "workspace",
        "model",
        "posture",
        "direction",
    };
    for (must_be_forbidden) |needle| {
        var found = false;
        for (semconv.METRIC_FORBIDDEN_ATTRS) |forbidden| {
            if (std.mem.eql(u8, needle, forbidden)) found = true;
        }
        try std.testing.expect(found);
    }
}

test "fixed attribute values are the exact pinned spellings" {
    try std.testing.expectEqualStrings("invoke_agent", semconv.OPERATION_INVOKE_AGENT); // pin test: literal is the contract
    try std.testing.expectEqualStrings("input", semconv.TokenType.input.label()); // pin test: literal is the contract
    try std.testing.expectEqualStrings("output", semconv.TokenType.output.label()); // pin test: literal is the contract
    try std.testing.expectEqualStrings("receive", semconv.ChargeClass.receive.label()); // pin test: literal is the contract
    try std.testing.expectEqualStrings("renewal", semconv.ChargeClass.renewal.label()); // pin test: literal is the contract
    try std.testing.expectEqualStrings("settle", semconv.ChargeClass.settle.label()); // pin test: literal is the contract
}
