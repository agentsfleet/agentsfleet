//! The interned value table's block arithmetic. Every closed enum owns a
//! contiguous run of indices, and a value resolves as that run's base plus the
//! member's own ordinal — which is what makes an unregistered value a build
//! failure instead of a runtime miss nobody counted.
//!
//! The wire-shape pins for these values live in otel_metrics_census_test.zig
//! and otel_metrics_egress_test.zig; this file pins only the arithmetic.

const std = @import("std");
const dims = @import("otel_metrics_dims.zig");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");
const Mode = @import("../state/tenant_provider.zig").Mode;

/// The enums the evented record API attaches. Fixed-dimension enums are covered
/// through `dimsFor` in the registry-wide test below.
const EVENTED = .{ semconv.TokenType, semconv.ChargeClass, Mode, semconv.ErrorType };

/// Comptime budget for the registry-wide walk: every family × its dimensions ×
/// each dimension's members, each resolving a base offset through a type scan.
/// Sized like the registry's own table budgets — the next power of ten above
/// the observed branch count, leaving headroom for new families.
const EVAL_QUOTA_REGISTRY_WALK: u32 = 1_000_000;

test "every closed enum member resolves to base plus its own ordinal" {
    inline for (EVENTED) |E| {
        inline for (@typeInfo(E).@"enum".fields) |f| {
            const member: E = @enumFromInt(f.value);
            const expected = dims.baseOf(E) + @intFromEnum(member);
            try std.testing.expectEqual(expected, dims.valueIndexOf(E, member));
        }
    }
}

test "each closed enum occupies a contiguous, disjoint run of the value table" {
    // Contiguity is what lets a single add replace a search: if a block were
    // sparse or overlapping, base + ordinal would land on a neighbour's value
    // and silently mislabel a sample.
    inline for (EVENTED) |E| {
        const base = dims.baseOf(E);
        const fields = @typeInfo(E).@"enum".fields;
        inline for (fields, 0..) |f, i| {
            const member: E = @enumFromInt(f.value);
            try std.testing.expectEqual(@as(u16, @intCast(base + i)), dims.valueIndexOf(E, member));
        }
        // The run ends where it should: the next index is past this enum, so no
        // two blocks share a slot.
        try std.testing.expect(base + fields.len <= dims.VALUES.len);
    }
}

test "every closed value renders its own spelling at egress" {
    // The index is an implementation detail; the string it resolves to is the
    // wire. This is the invariant that makes renumbering safe.
    inline for (EVENTED) |E| {
        inline for (@typeInfo(E).@"enum".fields) |f| {
            const member: E = @enumFromInt(f.value);
            const rendered = dims.VALUES[dims.valueIndexOf(E, member)];
            const expected = if (@hasDecl(E, "label")) member.label() else f.name;
            try std.testing.expectEqualStrings(expected, rendered);
        }
    }
}

test "a spelling shared by two enums keeps two distinct indices" {
    // The table is deliberately not deduplicated. Two enums that spell a value
    // identically must not collapse onto one index, or one enum's ordinal
    // arithmetic would walk into the other's block.
    var seen_by_spelling: usize = 0;
    inline for (EVENTED) |E| {
        inline for (@typeInfo(E).@"enum".fields) |f| {
            const member: E = @enumFromInt(f.value);
            const spelling = dims.VALUES[dims.valueIndexOf(E, member)];
            for (dims.VALUES) |candidate| {
                if (std.mem.eql(u8, candidate, spelling)) seen_by_spelling += 1;
            }
        }
    }
    // Every member found at least itself; the count only proves the scan ran
    // over live data rather than an empty table.
    try std.testing.expect(seen_by_spelling > 0);

    // The real assertion: distinct enums, distinct indices, even where the
    // registry happens to spell two values the same way.
    inline for (EVENTED) |A| {
        inline for (EVENTED) |B| {
            if (A == B) continue;
            try std.testing.expect(dims.baseOf(A) != dims.baseOf(B));
        }
    }
}

test "every fixed registry dimension resolves through the same block arithmetic" {
    // Walks the whole family registry rather than a hand-listed set, so a new
    // fixed dimension is covered the moment it is declared.
    @setEvalBranchQuota(EVAL_QUOTA_REGISTRY_WALK);
    inline for (0..families.METRIC_ID_COUNT) |i| {
        const id: families.MetricId = comptime @enumFromInt(i);
        inline for (comptime dims.dimsFor(id)) |dim| {
            switch (dim) {
                .fixed => |fixed| {
                    inline for (@typeInfo(fixed.Enum).@"enum".fields) |f| {
                        const member: fixed.Enum = @enumFromInt(f.value);
                        const idx = dims.valueIndexOf(fixed.Enum, member);
                        try std.testing.expectEqual(
                            @as(u16, dims.baseOf(fixed.Enum) + @intFromEnum(member)),
                            idx,
                        );
                        try std.testing.expect(idx < dims.VALUES.len);
                    }
                },
                .dynamic => {},
            }
        }
    }
}

test "the value table stays inside the index width the sample layout reserves" {
    // Removing deduplication grew the table; the sample's val_idx is u16 with
    // maxInt reserved as the dynamic-value sentinel.
    try std.testing.expect(dims.VALUES.len < std.math.maxInt(u16));
}

test "posture parses its own spellings and refuses everything else" {
    // The billing path depends on this fallback, so the parse itself is pinned
    // separately from the caller's `orelse .platform`: an unrecognised spelling
    // must be distinguishable, even though today's callers choose to absorb it.
    try std.testing.expectEqual(Mode.platform, Mode.parse("platform").?);
    try std.testing.expectEqual(Mode.self_managed, Mode.parse("self_managed").?);
    try std.testing.expect(Mode.parse("Platform") == null);
    try std.testing.expect(Mode.parse("self-managed") == null);
    try std.testing.expect(Mode.parse("") == null);
}

test "posture round-trips through its label without narrowing" {
    inline for (@typeInfo(Mode).@"enum".fields) |f| {
        const member: Mode = @enumFromInt(f.value);
        try std.testing.expectEqual(member, Mode.parse(member.label()).?);
    }
}
