const std = @import("std");

const S_FORMAT = "--format=";

pub const OutputFormat = enum {
    text,
    json,
};

pub const DoctorArgError = error{
    InvalidDoctorArgument,
    MissingFormatValue,
    InvalidFormatValue,
};

pub const DoctorOptions = struct {
    format: OutputFormat = .text,
    schema_gate: bool = false,
};

fn parseFormatValue(raw: []const u8) DoctorArgError!OutputFormat {
    if (std.mem.eql(u8, raw, "text")) return .text;
    if (std.mem.eql(u8, raw, "json")) return .json;
    return DoctorArgError.InvalidFormatValue;
}

pub fn parseDoctorArgs(args: []const []const u8) DoctorArgError!DoctorOptions {
    var out: DoctorOptions = .{};
    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--schema-gate")) {
            out.schema_gate = true;
            continue;
        }
        if (std.mem.eql(u8, arg, "--json")) {
            out.format = .json;
            continue;
        }
        if (std.mem.eql(u8, arg, "--format")) {
            if (i + 1 >= args.len) return DoctorArgError.MissingFormatValue;
            i += 1;
            out.format = try parseFormatValue(args[i]);
            continue;
        }
        if (std.mem.startsWith(u8, arg, S_FORMAT)) {
            out.format = try parseFormatValue(arg[S_FORMAT.len..]);
            continue;
        }
        return DoctorArgError.InvalidDoctorArgument;
    }
    return out;
}

test "parseDoctorArgs supports schema gate and json format" {
    const args = [_][]const u8{ "--schema-gate", "--format=json" };
    const parsed = try parseDoctorArgs(&args);
    try std.testing.expect(parsed.schema_gate);
    try std.testing.expectEqual(OutputFormat.json, parsed.format);
}

test "parseDoctorArgs rejects invalid arguments" {
    try std.testing.expectError(DoctorArgError.InvalidFormatValue, parseDoctorArgs(&[_][]const u8{ "--format", "yaml" }));
    try std.testing.expectError(DoctorArgError.MissingFormatValue, parseDoctorArgs(&[_][]const u8{"--format"}));
    try std.testing.expectError(DoctorArgError.InvalidDoctorArgument, parseDoctorArgs(&[_][]const u8{"--unknown"}));
}
