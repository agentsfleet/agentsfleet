//! `http.route` template resolution — totality and the no-caller-bytes rule.
//!
//! `templateFor` is the reason an HTTP span can carry a route at all. The pinned
//! conventions define `http.route` as the matched *template*, and this process
//! puts real workspace, fleet, lease, and secret identifiers in `req.url.path` —
//! so two properties have to hold for every route the router can produce, not
//! just the ones a hand-written table remembers:
//!
//!   1. **Total** — every `Route` variant resolves to a usable template. A new
//!      variant that forgets one would fail the exhaustive switch at compile
//!      time; this asserts the runtime shape (non-empty, absolute, no query).
//!   2. **Caller bytes never survive** — the returned slice is a compile-time
//!      literal, so a request's captured segments cannot reach the exporter.
//!      Proven behaviourally: the same variant carrying two different payloads
//!      must return the identical template, and neither payload's bytes may
//!      appear in it. That equality IS the low-cardinality guarantee — one route
//!      value per route, not one per request.
//!
//! The variants are enumerated by reflection rather than by hand precisely so a
//! route added tomorrow is covered without anyone remembering to add it here.

const std = @import("std");
const router = @import("router.zig");
const route_template = @import("route_template.zig");

/// Stand-ins for the identifiers a real request would capture. Two distinct
/// values so the template can be shown to be independent of the payload.
const CALLER_SUPPLIED_BYTES = "caller-supplied-first";
const ALTERNATE_CALLER_BYTES = "caller-supplied-second";

const PATH_SEPARATOR = '/';
const QUERY_DELIMITER = '?';
const FRAGMENT_DELIMITER = '#';

/// Build a route payload of any shape the `Route` union carries, filling every
/// string leaf with `bytes`. Reflection-driven so a new payload struct is
/// covered on the day it lands; an unhandled shape is a compile error rather
/// than a silently skipped variant.
fn payloadOf(comptime T: type, bytes: []const u8) T {
    if (T == void) return {};
    if (T == []const u8) return bytes;
    return switch (@typeInfo(T)) {
        // A decision enum is a fixed vocabulary, not caller text — any tag works.
        .@"enum" => |info| @enumFromInt(info.fields[0].value),
        .@"struct" => |info| blk: {
            // SAFETY: every field is assigned by the loop below before the value is read.
            var value: T = undefined;
            inline for (info.fields) |field| {
                @field(value, field.name) = payloadOf(field.type, bytes);
            }
            break :blk value;
        },
        else => @compileError("route payload needs a caller-shaped placeholder: " ++ @typeName(T)),
    };
}

fn templateOf(comptime field_name: []const u8, comptime T: type, bytes: []const u8) []const u8 {
    return route_template.templateFor(@unionInit(router.Route, field_name, payloadOf(T, bytes)));
}

test "test_route_template_is_total_and_absolute_for_every_route" {
    inline for (@typeInfo(router.Route).@"union".fields) |field| {
        const template = templateOf(field.name, field.type, CALLER_SUPPLIED_BYTES);

        if (template.len == 0) {
            std.debug.print("FAIL: route `{s}` resolved to an empty template\n", .{field.name});
            return error.EmptyRouteTemplate;
        }
        if (template[0] != PATH_SEPARATOR) {
            std.debug.print("FAIL: route `{s}` template `{s}` is not absolute\n", .{ field.name, template });
            return error.RouteTemplateNotAbsolute;
        }
        // A query string or fragment in a template would mean the exporter is
        // carrying request-specific data under a name that promises otherwise.
        for ([_]u8{ QUERY_DELIMITER, FRAGMENT_DELIMITER }) |forbidden| {
            if (std.mem.indexOfScalar(u8, template, forbidden) != null) {
                std.debug.print("FAIL: route `{s}` template `{s}` carries `{c}`\n", .{ field.name, template, forbidden });
                return error.RouteTemplateCarriesRequestTarget;
            }
        }
    }
}

test "test_route_template_never_echoes_caller_supplied_bytes" {
    inline for (@typeInfo(router.Route).@"union".fields) |field| {
        const first = templateOf(field.name, field.type, CALLER_SUPPLIED_BYTES);
        const second = templateOf(field.name, field.type, ALTERNATE_CALLER_BYTES);

        // Same route, different captured identifiers, identical template: the
        // backend sees one series per route rather than one per request.
        try std.testing.expectEqualStrings(first, second);

        for ([_][]const u8{ CALLER_SUPPLIED_BYTES, ALTERNATE_CALLER_BYTES }) |leaked| {
            if (std.mem.indexOf(u8, first, leaked) != null) {
                std.debug.print("FAIL: route `{s}` template `{s}` echoed caller bytes\n", .{ field.name, first });
                return error.RouteTemplateEchoesCallerBytes;
            }
        }
    }
}
