//! report_spool_entry.zig — what a spooled report is CALLED on disk, and which
//! directory entries are ours to replay.
//!
//! Extracted from `ReportSpool.zig` as the naming-and-classification concern
//! (Module Split Pattern: types + parsing). Both halves of the spool need these
//! rules — intake renders a name, replay recognises one — and neither should own
//! them, so they sit here with the tests that pin them. Every function is pure:
//! nothing here opens, reads or writes a file.

/// Held entries carry this suffix. Anything else in the spool belongs to an
/// operator, not to us, and is never posted.
pub const SUFFIX = ".report.json";

/// Entries no retry can settle move one level down into this directory, which
/// is a pile for a person to look at rather than a queue anything drains.
pub const QUARANTINE_DIR_NAME = "quarantine";

/// A lease id is canonical dashed UUID text (36 bytes). The cap is generous
/// rather than exact, so a future id spelling does not silently refuse to spool.
pub const LEASE_ID_MAX: usize = 64;
pub const NAME_MAX: usize = LEASE_ID_MAX + SUFFIX.len;
pub const QUARANTINE_PATH_MAX: usize = QUARANTINE_DIR_NAME.len + 1 + NAME_MAX;

/// Render `<lease_id>.report.json` into `buf`, or null when the id cannot be a
/// filename. This is a SECURITY check before it is a formatting one: the id
/// arrives over the wire, and a value carrying `/` or `..` would place a file
/// outside the spool. Only the characters a minted lease id uses are allowed,
/// so the question is never "which traversal spellings did we think of".
///
/// Borrows `lease_id`; the result borrows `buf`, which the caller owns.
pub fn name(buf: *[NAME_MAX]u8, lease_id: []const u8) ?[]const u8 {
    if (lease_id.len == 0 or lease_id.len > LEASE_ID_MAX) return null;
    for (lease_id) |c| {
        if (!std.ascii.isAlphanumeric(c) and c != DASH_CHAR) return null;
    }
    return std.fmt.bufPrint(buf, "{s}{s}", .{ lease_id, SUFFIX }) catch null;
}

/// Render the quarantine destination for an entry already named by `name`.
/// Borrows `entry`; the result borrows `buf`.
pub fn quarantinePath(buf: *[QUARANTINE_PATH_MAX]u8, entry: []const u8) ?[]const u8 {
    return std.fmt.bufPrint(buf, "{s}/{s}", .{ QUARANTINE_DIR_NAME, entry }) catch null;
}

/// True when a directory entry is one of ours to replay. The quarantine
/// directory fails on kind before the suffix is examined, and a file an operator
/// dropped in fails on the suffix, so neither is ever posted.
pub fn isHeld(entry: Dir.Entry) bool {
    if (entry.kind != .file) return false;
    if (entry.name.len > NAME_MAX) return false;
    return std.mem.endsWith(u8, entry.name, SUFFIX);
}

const std = @import("std");
const Dir = std.Io.Dir;

const DASH_CHAR: u8 = '-';

test "name refuses an id that could leave the spool" {
    var buf: [NAME_MAX]u8 = undefined;
    try std.testing.expectEqualStrings(
        "0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d05.report.json",
        name(&buf, "0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d05").?,
    );
    try std.testing.expect(name(&buf, "../../etc/passwd") == null);
    try std.testing.expect(name(&buf, "a/b") == null);
    try std.testing.expect(name(&buf, "..") == null);
    try std.testing.expect(name(&buf, ".") == null);
    try std.testing.expect(name(&buf, "") == null);
    try std.testing.expect(name(&buf, "x" ** (LEASE_ID_MAX + 1)) == null);
}

test "isHeld accepts a report and nothing else" {
    try std.testing.expect(isHeld(.{ .name = "0199a4c1.report.json", .kind = .file, .inode = 0 }));
    // The quarantine is a directory, so it fails on kind before the suffix.
    try std.testing.expect(!isHeld(.{ .name = QUARANTINE_DIR_NAME, .kind = .directory, .inode = 0 }));
    try std.testing.expect(!isHeld(.{ .name = "0199a4c1.report.json", .kind = .directory, .inode = 0 }));
    try std.testing.expect(!isHeld(.{ .name = "notes.txt", .kind = .file, .inode = 0 }));
    try std.testing.expect(!isHeld(.{ .name = "report.json.bak", .kind = .file, .inode = 0 }));
}

test "quarantinePath keeps the entry name under the pile" {
    var buf: [QUARANTINE_PATH_MAX]u8 = undefined;
    try std.testing.expectEqualStrings(
        "quarantine/0199a4c1.report.json",
        quarantinePath(&buf, "0199a4c1.report.json").?,
    );
}
