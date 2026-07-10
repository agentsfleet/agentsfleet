//! Unit tests for `sql_splitter.zig`, extracted to keep the source module
//! within the file-length cap. Force-imported from `tests.zig`.

const std = @import("std");
const sql_splitter = @import("sql_splitter.zig");
const SqlStatementSplitter = sql_splitter.SqlStatementSplitter;
const SplitError = sql_splitter.SplitError;

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

test "tagged dollar-quoted function body with internal semicolons is one statement" {
    var s = SqlStatementSplitter.init(
        \\CREATE FUNCTION f() RETURNS trigger AS $body$
        \\BEGIN
        \\  UPDATE t SET n = n + 1;
        \\  RAISE NOTICE 'inner; $$ stays inert';
        \\  RETURN NEW;
        \\END;
        \\$body$ LANGUAGE plpgsql;
    );
    const stmt = s.next().?;
    try std.testing.expect(std.mem.endsWith(u8, stmt, "LANGUAGE plpgsql"));
    try std.testing.expect(s.next() == null);
    try std.testing.expectEqual(@as(u32, 1), SqlStatementSplitter.count(
        "CREATE FUNCTION g() RETURNS void AS $fn_1$ BEGIN PERFORM 1; END; $fn_1$ LANGUAGE plpgsql;",
    ));
}

test "a different tag inside a tagged dollar-quote does not close it" {
    var s = SqlStatementSplitter.init("SELECT $outer$ text with $inner$ and ; inside $outer$;");
    try std.testing.expectEqualStrings("SELECT $outer$ text with $inner$ and ; inside $outer$", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "long and UTF-8 dollar-quote tags follow the Postgres lexer (no length cap)" {
    // A tag longer than any identifier cap is still one delimiter…
    const long_tag = "$a_tag_much_longer_than_sixty_three_characters_padding_padding_padding$";
    var s = SqlStatementSplitter.init("SELECT " ++ long_tag ++ " x; y " ++ long_tag ++ ";");
    try std.testing.expectEqualStrings("SELECT " ++ long_tag ++ " x; y " ++ long_tag, s.next().?);
    try std.testing.expect(s.next() == null);
    // …and high-bit (UTF-8) tag bytes are tag characters, not plain text.
    var u = SqlStatementSplitter.init("SELECT $état$ a ; b $état$;");
    try std.testing.expectEqualStrings("SELECT $état$ a ; b $état$", u.next().?);
    try std.testing.expect(u.next() == null);
}

test "positional parameters and lone dollars do not open a dollar-quote" {
    var s = SqlStatementSplitter.init("SELECT price, $1 FROM t WHERE cost > $2; SELECT 'a$b';");
    try std.testing.expectEqualStrings("SELECT price, $1 FROM t WHERE cost > $2", s.next().?);
    try std.testing.expectEqualStrings("SELECT 'a$b'", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "block comment with semicolons and dollar-quote markers is skipped entirely" {
    var s = SqlStatementSplitter.init(
        \\/* leading comment: ; $$ $body$ 'quote */
        \\CREATE TABLE t (id INT);
        \\SELECT /* inline; comment */ 1;
    );
    try std.testing.expectEqualStrings("CREATE TABLE t (id INT)", s.next().?);
    const stmt = s.next().?;
    try std.testing.expect(std.mem.startsWith(u8, stmt, "SELECT"));
    try std.testing.expect(std.mem.endsWith(u8, stmt, "1"));
    try std.testing.expect(s.next() == null);
}

test "nested block comments follow Postgres lexing" {
    var s = SqlStatementSplitter.init("/* outer /* inner; */ still comment; */ SELECT 1;");
    try std.testing.expectEqualStrings("SELECT 1", s.next().?);
    try std.testing.expect(s.next() == null);
}

test "validate rejects unterminated dollar-quote, block comment, and string" {
    try std.testing.expectError(
        SplitError.UnterminatedDollarQuote,
        SqlStatementSplitter.validate("CREATE FUNCTION f() AS $body$ BEGIN RETURN; END;"),
    );
    try std.testing.expectError(
        SplitError.UnterminatedBlockComment,
        SqlStatementSplitter.validate("SELECT 1; /* comment that never closes"),
    );
    try std.testing.expectError(
        SplitError.UnterminatedString,
        SqlStatementSplitter.validate("INSERT INTO t VALUES ('dangling);"),
    );
}

test "validate accepts terminated quoting in every supported form" {
    try SqlStatementSplitter.validate("SELECT 1;");
    try SqlStatementSplitter.validate("SELECT 'it''s; fine' FROM t; -- trailing comment");
    try SqlStatementSplitter.validate("/* c1 /* nested */ */ SELECT $tag$ body ; $$ $tag$;");
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
