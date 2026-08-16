//! Tests for runtime_validate: the shared secret predicates and the
//! ValidationError printer every boot failure routes through.

const std = @import("std");
const validate = @import("runtime_validate.zig");
const runtime_types = @import("runtime_types.zig");

const ValidationError = runtime_types.ValidationError;

test "isHexString accepts hex of either case and rejects the first stray byte" {
    try std.testing.expect(validate.isHexString("00deadBEEF19"));
    try std.testing.expect(validate.isHexString("")); // vacuously hex; length is the caller's check
    try std.testing.expect(!validate.isHexString("deadbeeg")); // 'g' just past the hex range
    try std.testing.expect(!validate.isHexString("dead beef")); // interior space
    try std.testing.expect(!validate.isHexString("0x00")); // the prefix spelling is not hex
}

test "isValid64HexKey binds length and charset together" {
    const good = "a1" ** 32;
    try std.testing.expect(validate.isValid64HexKey(good));
    // Right charset, wrong length — one short, one long.
    try std.testing.expect(!validate.isValid64HexKey(good[0..62]));
    try std.testing.expect(!validate.isValid64HexKey(good ++ "aa"));
    // Right length, wrong charset: last byte off the hex range. The doctor
    // green-lights with this predicate, so a divergence here is a key that
    // passes preflight and then refuses to boot.
    const bad_tail = ("a1" ** 31) ++ "zz";
    try std.testing.expect(!validate.isValid64HexKey(bad_tail));
}

test "printValidationError prints an operator line for every variant without dying" {
    // The printer is the LAST thing an operator sees before a refused boot, and
    // its switch is deliberately exhaustive so a new ValidationError variant
    // fails compilation until it gets a message. Driving every variant through
    // it proves none of the arms itself fatals or panics — fatalStderr prints
    // and returns; exiting is the boot path's job, not the printer's.
    inline for (@typeInfo(ValidationError).error_set.?) |variant| {
        validate.printValidationError(@field(ValidationError, variant.name));
    }
}
