//! Deterministic sample values for any wire type, by comptime reflection.
//!
//! `fixture_export.zig` decides WHAT to emit; this decides what a value of each
//! type looks like. Split for length, and because the two change for unrelated
//! reasons — a new wire type touches the emitter's roster, a new Zig type KIND
//! touches this.
//!
//! Values derive from field names: every string is its own field name, every
//! number a stable hash of it. Nothing is random or clock-derived, so
//! regenerating produces a byte-identical diff unless the wire genuinely
//! changed, and a field-order or field-name regression is obvious on sight.

const std = @import("std");

/// Longest a synthesized sample slice gets. One element proves the shape; more
/// only makes a diff harder to read.
const SAMPLE_ELEMENTS = 1;

/// FNV-1a constants. Any stable function would do — the requirement is that
/// regeneration is byte-identical unless the wire genuinely changed.
const FNV_OFFSET_BASIS: u32 = 2166136261;
const FNV_PRIME: u32 = 16777619;

/// Ceiling on a synthesized number, keeping fixtures readable.
const SAMPLE_NUMBER_RANGE = 1000;

/// Sampling runs inside a comptime-shaped recursion whose signature cannot carry
/// an error, so an exhausted arena aborts the generator rather than writing a
/// truncated fixture. A one-shot build tool is allowed to die.
const OOM_PANIC = "fixture_export: out of memory building a sample value";

/// Builds a deterministic sample value of `T`, naming it `field` for the
/// string and number derivations.
pub fn sample(comptime T: type, arena: std.mem.Allocator, comptime field: []const u8) T {
    // Checked before the switch: `std.json.Value` is a union whose first
    // variant is `null: void`, so dispatching on its type info would sample the
    // absence of a value rather than a free-form document.
    if (T == std.json.Value) return jsonValue(arena, field);
    return switch (@typeInfo(T)) {
        .void => {},
        .bool => true,
        .int => @intCast(@mod(hash(field), SAMPLE_NUMBER_RANGE) + 1),
        .float => 0.75,
        .@"enum" => @enumFromInt(0),
        .optional => |info| sample(info.child, arena, field),
        .@"struct" => sampleStruct(T, arena),
        .@"union" => sampleUnion(T, arena, field),
        .pointer => sampleSlice(T, arena, field),
        else => @compileError("fixture_export cannot sample " ++ @typeName(T)),
    };
}

fn sampleStruct(comptime T: type, arena: std.mem.Allocator) T {
    var value: T = undefined;
    inline for (@typeInfo(T).@"struct".fields) |f| {
        @field(value, f.name) = sample(f.type, arena, f.name);
    }
    return value;
}

/// One value per variant of a tagged union, in declaration order.
///
/// A union's own fixture carries all of them, the way an enum's carries every
/// tag: the payload encoding of a variant nothing sampled is exactly what drifts
/// unnoticed between two implementations.
pub fn allVariants(comptime T: type, arena: std.mem.Allocator) []const T {
    const fields = @typeInfo(T).@"union".fields;
    const out = arena.alloc(T, fields.len) catch @panic(OOM_PANIC);
    inline for (fields, 0..) |field, index| {
        out[index] = @unionInit(T, field.name, sample(field.type, arena, field.name));
    }
    return out;
}

/// The first variant of a tagged union, for a union used as a FIELD.
///
/// A struct instance can only carry one variant; the union's own fixture above
/// is what covers the rest.
fn sampleUnion(comptime T: type, arena: std.mem.Allocator, comptime field: []const u8) T {
    _ = field;
    const first = @typeInfo(T).@"union".fields[0];
    return @unionInit(T, first.name, sample(first.type, arena, first.name));
}

fn sampleSlice(comptime T: type, arena: std.mem.Allocator, comptime field: []const u8) T {
    const info = @typeInfo(T).pointer;
    if (info.child == u8) return field;
    const elements = arena.alloc(info.child, SAMPLE_ELEMENTS) catch @panic(OOM_PANIC);
    for (elements) |*element| element.* = sample(info.child, arena, field);
    return elements;
}

/// A one-key object for a free-form `std.json.Value` field.
fn jsonValue(arena: std.mem.Allocator, comptime field: []const u8) std.json.Value {
    const map = std.json.ObjectMap.init(
        arena,
        &.{field},
        &.{.{ .string = field }},
    ) catch @panic(OOM_PANIC);
    return .{ .object = map };
}

fn hash(comptime name: []const u8) u32 {
    return comptime blk: {
        var value: u32 = FNV_OFFSET_BASIS;
        for (name) |byte| {
            value ^= byte;
            value *%= FNV_PRIME;
        }
        break :blk value;
    };
}
