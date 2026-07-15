const std = @import("std");
const validate = @import("validate.zig");

test "validate: accepts bounded five-field cron expressions" {
    try validate.cron("0 9 * * *");
    try validate.cron("*/15 0-23 * * 1-5");
    try validate.cron("0,30 8,12,17 1-31/2 1,6,12 0,7");
    try validate.cron("  0  9  *  *  *  ");
}

test "validate: rejects malformed field count and unsupported syntax" {
    inline for (&.{ "", "* * *", "* * * *", "* * * * * *", "@daily", "0 9 ? * *", "0 9 L * *" }) |value| {
        try std.testing.expectError(error.InvalidCron, validate.cron(value));
    }
}

test "validate: rejects numbers outside each cron field" {
    inline for (&.{ "60 * * * *", "* 24 * * *", "* * 0 * *", "* * 32 * *", "* * * 0 *", "* * * 13 *", "* * * * 8" }) |value| {
        try std.testing.expectError(error.InvalidCron, validate.cron(value));
    }
}

test "validate: rejects broken lists ranges and steps" {
    inline for (&.{ "1,,2 * * * *", "5-1 * * * *", "*/0 * * * *", "*/61 * * * *", "1/2/3 * * * *", "-1 * * * *", "1- * * * *" }) |value| {
        try std.testing.expectError(error.InvalidCron, validate.cron(value));
    }
}

test "validate: rejects cron expressions above the storage bound" {
    const too_long = "1" ** (validate.MAX_CRON_LEN + 1);
    try std.testing.expectError(error.InvalidCron, validate.cron(too_long));
}

test "validate: accepts timezone shapes used by QStash" {
    try validate.timezone("UTC");
    try validate.timezone("Asia/Kolkata");
    try validate.timezone("America/Argentina/Buenos_Aires");
    try validate.timezone("Etc/GMT+5");
}

test "validate: rejects unsafe or empty timezone shapes" {
    inline for (&.{ "", "/UTC", "Asia/", "Asia//Kolkata", "../UTC", "Asia/Kolkata?token=x", "Asia Kolkata" }) |value| {
        try std.testing.expectError(error.InvalidTimezone, validate.timezone(value));
    }
}

test "validate: enforces timezone length bound" {
    const too_long = "A" ** (validate.MAX_TIMEZONE_LEN + 1);
    try std.testing.expectError(error.InvalidTimezone, validate.timezone(too_long));
}

test "validate: requires a bounded non-whitespace message" {
    try validate.message("summarize today's Zoho Sprints");
    inline for (&.{ "", "   ", "\n\t" }) |value| {
        try std.testing.expectError(error.InvalidMessage, validate.message(value));
    }
    const too_long = "x" ** (validate.MAX_MESSAGE_LEN + 1);
    try std.testing.expectError(error.InvalidMessage, validate.message(too_long));
}
