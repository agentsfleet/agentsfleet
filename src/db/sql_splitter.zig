//! SqlStatementSplitter — splits raw SQL text into individual statements.
//!
//! Handles:
//!   - `;` as statement terminator
//!   - `'...'` single-quoted string literals (with `''` escape)
//!   - `$$...$$` bare dollar-quoting (tagged dollar-quotes not supported)
//!   - `-- ...` line comments (stripped from output)
//!
//! Does NOT handle:
//!   - `/* ... */` block comments
//!   - Tagged dollar-quotes (e.g. `$body$...$body$`)

const std = @import("std");

pub const SqlStatementSplitter = struct {
    sql: []const u8,
    pos: usize,
    in_single_quote: bool,
    in_dollar_quote: bool,

    pub fn init(sql: []const u8) SqlStatementSplitter {
        return .{
            .sql = sql,
            .pos = 0,
            .in_single_quote = false,
            .in_dollar_quote = false,
        };
    }

    /// Advance past whitespace and line comments. Returns the position of
    /// the first non-comment, non-whitespace character (or end of input).
    fn skipWhitespaceAndComments(self: *SqlStatementSplitter) void {
        while (self.pos < self.sql.len) {
            const ch = self.sql[self.pos];
            if (ch == ' ' or ch == '\t' or ch == '\r' or ch == '\n') {
                self.pos += 1;
                continue;
            }
            if (ch == '-' and self.pos + 1 < self.sql.len and self.sql[self.pos + 1] == '-') {
                while (self.pos < self.sql.len and self.sql[self.pos] != '\n') : (self.pos += 1) {}
                continue;
            }
            break;
        }
    }

    /// Returns the next non-empty, trimmed SQL statement, or null when exhausted.
    pub fn next(self: *SqlStatementSplitter) ?[]const u8 {
        self.skipWhitespaceAndComments();
        if (self.pos >= self.sql.len) return null;

        const start = self.pos;

        while (self.pos < self.sql.len) {
            const ch = self.sql[self.pos];

            // Skip inline -- comments within a statement.
            if (!self.in_single_quote and !self.in_dollar_quote and
                ch == '-' and self.pos + 1 < self.sql.len and self.sql[self.pos + 1] == '-')
            {
                while (self.pos < self.sql.len and self.sql[self.pos] != '\n') : (self.pos += 1) {}
                continue;
            }

            // Track single-quoted string literals.
            if (!self.in_dollar_quote and ch == '\'') {
                if (self.in_single_quote and self.pos + 1 < self.sql.len and self.sql[self.pos + 1] == '\'') {
                    self.pos += 2;
                    continue;
                }
                self.in_single_quote = !self.in_single_quote;
                self.pos += 1;
                continue;
            }

            // Track bare $$ dollar-quoting.
            if (!self.in_single_quote and self.pos + 1 < self.sql.len and
                ch == '$' and self.sql[self.pos + 1] == '$')
            {
                self.in_dollar_quote = !self.in_dollar_quote;
                self.pos += 2;
                continue;
            }

            // Statement terminator — only outside quotes.
            if (ch == ';' and !self.in_single_quote and !self.in_dollar_quote) {
                const stmt = std.mem.trim(u8, self.sql[start..self.pos], " \t\r\n");
                self.pos += 1;
                if (stmt.len > 0) return stmt;
                return self.next();
            }

            self.pos += 1;
        }

        // Tail — content after last semicolon.
        const tail = std.mem.trim(u8, self.sql[start..], " \t\r\n");
        if (tail.len > 0) return tail;
        return null;
    }

    /// Count total statements without side effects.
    pub fn count(sql: []const u8) u32 {
        var splitter = SqlStatementSplitter.init(sql);
        var n: u32 = 0;
        while (splitter.next() != null) : (n += 1) {}
        return n;
    }
};

// ── Tests ──────────────────────────────────────────────────────────────

test "splits simple statements on semicolons" {
    var s = SqlStatementSplitter.init("CREATE TABLE t (id INT); INSERT INTO t VALUES (1);");
    try std.testing.expectEqualStrings("CREATE TABLE t (id INT)", s.next().?);
    try std.testing.expectEqualStrings("INSERT INTO t VALUES (1)", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "preserves semicolons inside single-quoted strings" {
    var s = SqlStatementSplitter.init("INSERT INTO t VALUES ('hello; world');");
    try std.testing.expectEqualStrings("INSERT INTO t VALUES ('hello; world')", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "handles escaped single quotes" {
    var s = SqlStatementSplitter.init("INSERT INTO t VALUES ('it''s ok');");
    try std.testing.expectEqualStrings("INSERT INTO t VALUES ('it''s ok')", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "preserves semicolons inside dollar-quoted blocks" {
    var s = SqlStatementSplitter.init(
        \\CREATE FUNCTION f() RETURNS void AS $$
        \\BEGIN
        \\  RAISE NOTICE 'done;';
        \\END;
        \\$$ LANGUAGE plpgsql;
    );
    const stmt = s.next().?;
    try std.testing.expect(std.mem.containsAtLeast(u8, stmt, 1, "RAISE NOTICE"));
    try std.testing.expect(s.next() == null);
}

test "skips leading -- line comments" {
    var s = SqlStatementSplitter.init(
        \\-- This is a comment with ; and ' characters
        \\SELECT 1;
    );
    try std.testing.expectEqualStrings("SELECT 1", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "apostrophe in comment does not open string literal" {
    var s = SqlStatementSplitter.init(
        \\-- This slot's existence matters; don't remove
        \\SELECT 1;
    );
    try std.testing.expectEqualStrings("SELECT 1", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "comment-only input returns null" {
    var s = SqlStatementSplitter.init(
        \\-- version marker only
        \\-- no tables here
    );
    try std.testing.expect(s.next() == null);
}

test "version marker file: comments + SELECT 1" {
    var s = SqlStatementSplitter.init(
        \\-- removed_table.sql
        \\-- Slot reserved; original table dropped. This slot's existence matters.
        \\SELECT 1;
    );
    try std.testing.expectEqualStrings("SELECT 1", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "multiple statements with interleaved comments" {
    var s = SqlStatementSplitter.init(
        \\-- Create schema
        \\CREATE SCHEMA IF NOT EXISTS core;
        \\-- Create table
        \\CREATE TABLE core.t (id INT);
        \\-- Done
    );
    try std.testing.expectEqualStrings("CREATE SCHEMA IF NOT EXISTS core", s.next().?);
    try std.testing.expectEqualStrings("CREATE TABLE core.t (id INT)", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "empty input returns null" {
    var s = SqlStatementSplitter.init("");
    try std.testing.expect(s.next() == null);
}

test "whitespace-only input returns null" {
    var s = SqlStatementSplitter.init("  \n\t\n  ");
    try std.testing.expect(s.next() == null);
}

test "trailing content without semicolon is returned" {
    var s = SqlStatementSplitter.init("SELECT 1; SELECT 2");
    try std.testing.expectEqualStrings("SELECT 1", s.next().?);
    try std.testing.expectEqualStrings("SELECT 2", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "count returns correct number of statements" {
    try std.testing.expectEqual(@as(u32, 3), SqlStatementSplitter.count("A; B; C;"));
    try std.testing.expectEqual(@as(u32, 1), SqlStatementSplitter.count("SELECT 1;"));
    try std.testing.expectEqual(@as(u32, 0), SqlStatementSplitter.count("-- comment only"));
    try std.testing.expectEqual(@as(u32, 1), SqlStatementSplitter.count("-- comment\nSELECT 1;"));
}

test "inline comment after SQL is included in statement" {
    var s = SqlStatementSplitter.init("SELECT 1 -- trailing comment\n;");
    const stmt = s.next().?;
    // The comment is part of the statement text (between start and ;)
    // Postgres handles it fine — it strips comments during parsing.
    try std.testing.expect(std.mem.startsWith(u8, stmt, "SELECT 1"));
    try std.testing.expect(s.next() == null);
}
