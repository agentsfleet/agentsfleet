//! Emits the canonical wire fixtures the Rust port proves itself against.
//!
//! Zig is the source of truth for the `/v1/runners` wire. This tool writes one
//! canonical JSON document per exported wire type into a directory, plus a
//! `manifest.json` describing what it wrote, and the Rust `afd_wire` suite
//! deserializes and re-serializes each one and compares BYTES. The comparison
//! only means something if the byte form is defined rather than incidental, so
//! this file defines it: minified (no insignificant whitespace), fields in
//! declaration order, optionals present and populated, enums rendered by tag
//! name.
//!
//! Run it through `make wire-fixtures`, which reports what landed — this tool
//! is silent on success and fails loudly, so its console surface is the recipe's
//! rather than a log line the logging gate would rightly object to.
//!
//! Run it through `make wire-fixtures`. It imports the contract modules by
//! sibling path, so it compiles under `zig run` with no entry in the build
//! graph — this tool exists to prove the Rust port, not to change the Zig one.
//!
//! The roster is comptime reflection over what the contract modules actually
//! export, because a hand-written list is a list someone forgets to update and
//! a forgotten wire type is the drift these fixtures exist to catch. Only two
//! things are hand-maintained, being what reflection cannot know: which modules
//! are deliberately excluded, and which types the daemon parses leniently.
//!
//! Sample values derive from field names — every string is its own field name,
//! every number a stable hash of it. Nothing is random or clock-derived, so
//! regeneration is a no-op diff unless the wire genuinely changed.

const std = @import("std");

const contract = @import("contract.zig");

/// Why a module contract.zig exports produces no fixture.
const Skip = enum {
    /// A superseded shape the Rust port deliberately does not implement.
    superseded,
    /// Carries conversions or constants, no wire type of its own.
    no_wire_type,
};

const Excluded = struct { module: []const u8, why: Skip, reason: []const u8 };

/// Modules `contract.zig` re-exports that emit no fixture, each with its reason.
///
/// `protocol_lease_v1` is the load-bearing entry. Without it, "every exported
/// type gets a fixture" would emit a version-one lease and the Rust port would
/// grow a version-one serde type to round-trip it — compatibility arriving
/// through the back door. Indy's call, Aug 23, 2026: the port implements the
/// current shape only. The evidence is that both wire-version constants were
/// introduced in one commit, so "version one" names a pre-existing shape rather
/// than a designed protocol, and no in-tree code path emits it — the runner
/// posts the current request unconditionally. The Zig daemon keeps its
/// version-one path and retires with it; nothing is deleted here.
const EXCLUDED = [_]Excluded{
    .{
        .module = "protocol_lease_v1",
        .why = .superseded,
        .reason = "superseded lease shape; no in-tree emitter, the port implements the current shape only",
    },
    .{
        .module = "report_mapping",
        .why = .no_wire_type,
        .reason = "one conversion between execution_result and the report wire; both endpoints are covered",
    },
};

/// Types the daemon parses with `ignore_unknown_fields = true`.
///
/// The policy is per call site, not global — a blanket setting either way would
/// be wrong — so this mirrors what the parsers actually do. Anything absent
/// rejects an unknown field, which is `std.json`'s default and the stricter
/// half. The Rust serde attributes mirror this list through the manifest.
const LENIENT = [_][]const u8{
    "LeaseRequest",      "LeaseResponse",  "LeasePayload",
    "HeartbeatRequest",  "AssignedPolicy", "CapabilityReport",
    "SelftestReport",    "SelftestCheck",  "ExtraBind",
    "MemoryPushRequest", "RenewRequest",
};

/// Where the fixtures land, relative to the repository root.
///
/// A constant rather than an argument: there is exactly one canonical location,
/// the Rust suite reads that same path, and a generator that can be pointed
/// somewhere else is a generator that can silently write fixtures nothing
/// checks. `make wire-fixtures` runs this from the repository root.
const OUTPUT_DIR = "samples/fixtures/wire-v2";

/// Longest a synthesized sample slice gets. One element proves the shape; more
/// only makes a diff harder to read.
const SAMPLE_ELEMENTS = 1;

/// FNV-1a constants. Any stable function would do — the requirement is that
/// regeneration is byte-identical unless the wire genuinely changed.
const FNV_OFFSET_BASIS: u32 = 2166136261;
const FNV_PRIME: u32 = 16777619;

/// Ceiling on a synthesized number, keeping fixtures readable.
const SAMPLE_NUMBER_RANGE = 1000;

/// Sampling runs inside a comptime-shaped recursion whose signature cannot
/// carry an error, so an exhausted arena aborts the generator rather than
/// writing a truncated fixture. A one-shot build tool is allowed to die.
const OOM_PANIC = "fixture_export: out of memory building a sample value";

pub fn main() !void {
    var debug_allocator: std.heap.DebugAllocator(.{}) = .init;
    defer std.debug.assert(debug_allocator.deinit() == .ok);
    const gpa = debug_allocator.allocator();

    var arena_state = std.heap.ArenaAllocator.init(gpa);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const io = std.Io.Threaded.global_single_threaded.io();
    var sink: Sink = .{
        .arena = arena,
        .io = io,
        .dir = try openOutputDir(io, OUTPUT_DIR),
        .entries = .empty,
    };
    defer sink.dir.close(io);

    try emitAll(&sink);
    try writeManifest(&sink);
}

/// One manifest row: what was written and how the daemon parses it.
const Entry = struct {
    name: []const u8,
    module: []const u8,
    kind: []const u8,
    unknown_fields: []const u8,
    file: []const u8,
};

/// Creates `OUTPUT_DIR` component by component and opens it.
///
/// Zig 0.16 moved the filesystem behind `std.Io` and dropped the
/// create-all-parents helper, so the walk is explicit. An already-present
/// component is the normal case — the fixtures are regenerated in place.
fn openOutputDir(io: std.Io, path: []const u8) !std.Io.Dir {
    const cwd = std.Io.Dir.cwd();
    var walked: usize = 0;
    while (std.mem.indexOfScalarPos(u8, path, walked, '/')) |slash| {
        try createIfAbsent(cwd, io, path[0..slash]);
        walked = slash + 1;
    }
    try createIfAbsent(cwd, io, path);
    return cwd.openDir(io, path, .{});
}

fn createIfAbsent(dir: std.Io.Dir, io: std.Io, path: []const u8) !void {
    dir.createDir(io, path, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };
}

/// Walks every module `contract.zig` exports, skipping the declared exclusions,
/// and emits one fixture per exported struct, enum or tagged union.
/// Everything writing a fixture needs, so a signature stays one line.
const Sink = struct {
    arena: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    entries: std.ArrayList(Entry),
};

fn emitAll(sink: *Sink) !void {
    inline for (@typeInfo(contract).@"struct".decls) |module_decl| {
        const module = @field(contract, module_decl.name);
        if (@TypeOf(module) == type and !isExcluded(module_decl.name)) {
            try emitModule(sink, module_decl.name, module);
        }
    }
}

fn emitModule(sink: *Sink, comptime module_name: []const u8, comptime module: type) !void {
    inline for (@typeInfo(module).@"struct".decls) |decl| {
        const value = @field(module, decl.name);
        if (@TypeOf(value) == type) {
            switch (@typeInfo(value)) {
                .@"struct", .@"enum", .@"union" => try emitType(sink, module_name, decl.name, value),
                else => {},
            }
        }
    }
}

fn emitType(
    sink: *Sink,
    comptime module_name: []const u8,
    comptime type_name: []const u8,
    comptime T: type,
) !void {
    const qualified = module_name ++ "." ++ type_name;
    const file_name = qualified ++ ".json";

    const body = if (@typeInfo(T) == .@"enum")
        try std.json.Stringify.valueAlloc(sink.arena, allTags(T), .{})
    else
        try std.json.Stringify.valueAlloc(sink.arena, sample(T, sink.arena, type_name), .{});

    try sink.dir.writeFile(sink.io, .{ .sub_path = file_name, .data = body });
    try sink.entries.append(sink.arena, .{
        .name = qualified,
        .module = module_name,
        .kind = if (@typeInfo(T) == .@"enum") "enum" else "object",
        .unknown_fields = if (isLenient(type_name)) "ignore" else "reject",
        .file = file_name,
    });
}

/// Every tag of an enum, in declaration order.
///
/// An enum fixture is the whole vocabulary rather than one sampled value: the
/// wire spelling of a variant is exactly the kind of thing that drifts between
/// two implementations, and a fixture carrying one variant proves only that one.
fn allTags(comptime T: type) []const []const u8 {
    return comptime blk: {
        var tags: []const []const u8 = &.{};
        for (@typeInfo(T).@"enum".fields) |field| tags = tags ++ .{field.name};
        break :blk tags;
    };
}

/// Builds a deterministic sample value of `T`, naming it `field` for the
/// string and number derivations.
fn sample(comptime T: type, arena: std.mem.Allocator, comptime field: []const u8) T {
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

/// The first variant of a tagged union. `std.json` renders it `{"tag": value}`,
/// which is the discriminator the wire carries.
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

fn isExcluded(comptime module_name: []const u8) bool {
    return comptime blk: {
        for (EXCLUDED) |entry| {
            if (std.mem.eql(u8, entry.module, module_name)) break :blk true;
        }
        break :blk false;
    };
}

fn isLenient(comptime type_name: []const u8) bool {
    return comptime blk: {
        for (LENIENT) |name| {
            if (std.mem.eql(u8, name, type_name)) break :blk true;
        }
        break :blk false;
    };
}

/// Describes what was written, so the Rust suite asserts against a machine
/// readable list rather than a hand-kept count, and so the exclusions are data
/// a test can check rather than prose in a comment.
fn writeManifest(sink: *Sink) !void {
    const arena = sink.arena;
    const Manifest = struct {
        wire_version: u16,
        types: []const Entry,
        excluded: []const struct {
            module: []const u8,
            why: []const u8,
            reason: []const u8,
        },
    };

    var excluded = try arena.alloc(
        @typeInfo(@FieldType(Manifest, "excluded")).pointer.child,
        EXCLUDED.len,
    );
    for (EXCLUDED, 0..) |entry, index| {
        excluded[index] = .{
            .module = entry.module,
            .why = @tagName(entry.why),
            .reason = entry.reason,
        };
    }

    const body = try std.json.Stringify.valueAlloc(arena, Manifest{
        .wire_version = contract.protocol.LEASE_WIRE_VERSION_CURRENT,
        .types = sink.entries.items,
        .excluded = excluded,
    }, .{ .whitespace = .indent_2 });
    try sink.dir.writeFile(sink.io, .{ .sub_path = "manifest.json", .data = body });
}
