const std = @import("std");
const validate = @import("runtime_validate.zig");
const runtime = @import("runtime.zig");

// Compile-time smoke check: the façade alias `ServeConfig.printValidationError`
// must keep resolving to validate.printValidationError. If a future rename in
// runtime_validate.zig drops the symbol, this assertion fails to build —
// callers like src/cmd/serve.zig depend on the static-method form.
comptime {
    std.debug.assert(runtime.ServeConfig.printValidationError == validate.printValidationError);
}

test "isHexString validates encryption key format" {
    try std.testing.expect(validate.isHexString("abcdef0123"));
    try std.testing.expect(!validate.isHexString("abcxyz"));
    try std.testing.expect(validate.isHexString("")); // vacuously true; load() length-checks separately
}
