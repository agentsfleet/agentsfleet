//! SqlStatementSplitter — splits raw SQL text into individual statements.
//!
//! Handles, per the Postgres lexer:
//!   - `;` as statement terminator
//!   - `'...'` strings (`''` escape) and `E'...'` escape strings (backslash
//!     escapes honored, so `\'` stays inside the literal)
//!   - `"..."` quoted identifiers (`""` escape) — `;`, `--`, `/*`, `'` inside
//!     them are inert
//!   - `$$...$$` and tagged `$tag$...$tag$` dollar-quoting (UTF-8 tag bytes,
//!     no length cap; a delimiter never opens immediately after an identifier
//!     byte — maximal munch folds that `$` into the identifier)
//!   - `-- ...` line comments and `/* ... */` block comments (nesting) —
//!     neither contributes a statement boundary
//!
//! `validate` is the loud backstop: input that ends inside a string,
//! identifier, dollar-quote, or block comment returns a named `SplitError` so
//! a malformed migration fails before apply instead of splitting on a boundary
//! inside the unterminated region. The error set is distinct from `error.PG`
//! so callers never conflate a malformed migration with a database failure.
//!
//! Tests live in `sql_splitter_test.zig` (force-imported from `tests.zig`).

const std = @import("std");

const WHITESPACE_CHARS = " \t\r\n";
const LINE_COMMENT_MARKER = "--";
const BLOCK_COMMENT_OPEN = "/*";
const BLOCK_COMMENT_CLOSE = "*/";
const DOLLAR_QUOTE_CHAR: u8 = '$';
const SINGLE_QUOTE_CHAR: u8 = '\'';
const DOUBLE_QUOTE_CHAR: u8 = '"';
const BACKSLASH_CHAR: u8 = '\\';
/// Postgres allows high-bit bytes in dollar-quote tags (lexer pattern
/// `\$([A-Za-z\200-\377_][A-Za-z\200-\377_0-9]*)?\$`), so UTF-8 tags work.
const HIGH_BIT_CHAR_FLOOR: u8 = 0x80;

pub const SplitError = error{
    UnterminatedString,
    UnterminatedQuotedIdentifier,
    UnterminatedDollarQuote,
    UnterminatedBlockComment,
};

fn isDollarTagStartChar(c: u8) bool {
    return std.ascii.isAlphabetic(c) or c == '_' or c >= HIGH_BIT_CHAR_FLOOR;
}

fn isDollarTagChar(c: u8) bool {
    return isDollarTagStartChar(c) or std.ascii.isDigit(c);
}

/// Postgres `ident_cont`: `[A-Za-z0-9_$\200-\377]`. A dollar-quote delimiter
/// cannot immediately follow one of these — maximal munch makes `a$b$` a
/// single identifier, not `a` + an opening `$b$`.
fn isIdentifierByte(c: u8) bool {
    return isDollarTagChar(c) or c == DOLLAR_QUOTE_CHAR;
}

/// Dollar-quote delimiter at `pos`: `$$` or `$tag$` (no tag length cap — the
/// scan stops at the first non-tag byte). Returns the whole delimiter
/// including both `$` so the closer can be matched verbatim, or null when
/// `pos` starts a positional parameter (`$1`) or a lone `$`.
fn dollarDelimiterAt(sql: []const u8, pos: usize) ?[]const u8 {
    std.debug.assert(sql[pos] == DOLLAR_QUOTE_CHAR);
    var i = pos + 1;
    while (i < sql.len) : (i += 1) {
        const c = sql[i];
        if (c == DOLLAR_QUOTE_CHAR) return sql[pos .. i + 1];
        const valid_tag_char = if (i == pos + 1) isDollarTagStartChar(c) else isDollarTagChar(c);
        if (!valid_tag_char) return null;
    }
    return null;
}

pub const SqlStatementSplitter = struct {
    const Self = @This();

    sql: []const u8,
    pos: usize,
    in_single_quote: bool,
    in_double_quote: bool,
    /// True while inside an `E'...'` string, where backslash escapes (incl.
    /// `\'`) stay inside the literal; a plain `'...'` treats `\` literally
    /// (standard_conforming_strings).
    single_quote_backslash_escapes: bool,
    /// Whole opening delimiter (`$$`, `$body$`) while inside a dollar-quote;
    /// the closing delimiter must match it verbatim, so `;`, `$$`, and other
    /// tags inside the quoted body stay inert.
    dollar_tag: ?[]const u8,
    /// Set when a `/*` ran to end of input unterminated; `validate` reports it
    /// (the comment itself was consumed, so `next` can't signal the defect).
    unterminated_block_comment: bool,

    pub fn init(sql: []const u8) SqlStatementSplitter {
        return .{
            .sql = sql,
            .pos = 0,
            .in_single_quote = false,
            .in_double_quote = false,
            .single_quote_backslash_escapes = false,
            .dollar_tag = null,
            .unterminated_block_comment = false,
        };
    }

    fn matchesAt(self: *const Self, marker: []const u8) bool {
        return std.mem.startsWith(u8, self.sql[self.pos..], marker);
    }

    fn skipLineComment(self: *Self) void {
        while (self.pos < self.sql.len and self.sql[self.pos] != '\n') : (self.pos += 1) {}
    }

    /// Postgres block comments nest; consume through the matching close. An
    /// unterminated comment consumes to end of input and flags itself.
    fn skipBlockComment(self: *Self) void {
        var depth: u32 = 0;
        while (self.pos < self.sql.len) {
            if (self.matchesAt(BLOCK_COMMENT_OPEN)) {
                depth += 1;
                self.pos += BLOCK_COMMENT_OPEN.len;
            } else if (self.matchesAt(BLOCK_COMMENT_CLOSE)) {
                depth -= 1;
                self.pos += BLOCK_COMMENT_CLOSE.len;
                if (depth == 0) return;
            } else {
                self.pos += 1;
            }
        }
        self.unterminated_block_comment = true;
    }

    /// Advance past whitespace and comments between statements, so a leading
    /// comment is excluded from the next statement's slice.
    fn skipWhitespaceAndComments(self: *Self) void {
        while (self.pos < self.sql.len) {
            const ch = self.sql[self.pos];
            if (ch == ' ' or ch == '\t' or ch == '\r' or ch == '\n') {
                self.pos += 1;
                continue;
            }
            if (self.matchesAt(LINE_COMMENT_MARKER)) {
                self.skipLineComment();
                continue;
            }
            if (self.matchesAt(BLOCK_COMMENT_OPEN)) {
                self.skipBlockComment();
                continue;
            }
            break;
        }
    }

    /// The `'` opening at `pos` starts an `E'...'` escape string when a
    /// standalone `E`/`e` immediately precedes it (standalone = not itself
    /// the tail of an identifier like `CASE` or `table_e`).
    fn escapeStringPrefixAt(self: *const Self, pos: usize) bool {
        if (pos == 0) return false;
        const prev = self.sql[pos - 1];
        if (prev != 'E' and prev != 'e') return false;
        return pos == 1 or !isIdentifierByte(self.sql[pos - 2]);
    }

    fn consumeSingleQuoteByte(self: *Self, ch: u8) void {
        if (self.single_quote_backslash_escapes and ch == BACKSLASH_CHAR and self.pos + 1 < self.sql.len) {
            self.pos += 2; // E'...': the escaped byte (incl. `\'`) stays inside
            return;
        }
        if (ch == SINGLE_QUOTE_CHAR) {
            if (self.pos + 1 < self.sql.len and self.sql[self.pos + 1] == SINGLE_QUOTE_CHAR) {
                self.pos += 2; // `''` escape stays inside the literal
                return;
            }
            self.in_single_quote = false;
        }
        self.pos += 1;
    }

    fn consumeDoubleQuoteByte(self: *Self, ch: u8) void {
        if (ch == DOUBLE_QUOTE_CHAR) {
            if (self.pos + 1 < self.sql.len and self.sql[self.pos + 1] == DOUBLE_QUOTE_CHAR) {
                self.pos += 2; // `""` escape stays inside the identifier
                return;
            }
            self.in_double_quote = false;
        }
        self.pos += 1;
    }

    /// Returns the next non-empty, trimmed SQL statement, or null when exhausted.
    pub fn next(self: *Self) ?[]const u8 {
        // A loop, not recursion: a long run of empty statements (`;;;;…`) must
        // not grow the stack — validate() walks this same path on untrusted
        // input and must fail with a named error, never a stack overflow.
        scan: while (true) {
            self.skipWhitespaceAndComments();
            if (self.pos >= self.sql.len) return null;

            const start = self.pos;

            while (self.pos < self.sql.len) {
                const ch = self.sql[self.pos];

                if (self.dollar_tag) |tag| {
                    if (ch == DOLLAR_QUOTE_CHAR and self.matchesAt(tag)) {
                        self.dollar_tag = null;
                        self.pos += tag.len;
                    } else {
                        self.pos += 1;
                    }
                    continue;
                }
                if (self.in_single_quote) {
                    self.consumeSingleQuoteByte(ch);
                    continue;
                }
                if (self.in_double_quote) {
                    self.consumeDoubleQuoteByte(ch);
                    continue;
                }

                if (self.matchesAt(LINE_COMMENT_MARKER)) {
                    self.skipLineComment();
                    continue;
                }
                if (self.matchesAt(BLOCK_COMMENT_OPEN)) {
                    self.skipBlockComment();
                    continue;
                }
                if (ch == SINGLE_QUOTE_CHAR) {
                    self.in_single_quote = true;
                    self.single_quote_backslash_escapes = self.escapeStringPrefixAt(self.pos);
                    self.pos += 1;
                    continue;
                }
                if (ch == DOUBLE_QUOTE_CHAR) {
                    self.in_double_quote = true;
                    self.pos += 1;
                    continue;
                }
                if (ch == DOLLAR_QUOTE_CHAR and (self.pos == 0 or !isIdentifierByte(self.sql[self.pos - 1]))) {
                    if (dollarDelimiterAt(self.sql, self.pos)) |tag| {
                        self.dollar_tag = tag;
                        self.pos += tag.len;
                        continue;
                    }
                }
                if (ch == ';') {
                    const stmt = std.mem.trim(u8, self.sql[start..self.pos], WHITESPACE_CHARS);
                    self.pos += 1;
                    if (stmt.len > 0) return stmt;
                    continue :scan; // empty statement — rescan without recursing
                }

                self.pos += 1;
            }

            const tail = std.mem.trim(u8, self.sql[start..], WHITESPACE_CHARS);
            if (tail.len > 0) return tail;
            return null;
        }
    }

    /// Count total statements without side effects.
    pub fn count(sql: []const u8) u32 {
        var splitter = SqlStatementSplitter.init(sql);
        var n: u32 = 0;
        while (splitter.next() != null) : (n += 1) {}
        return n;
    }

    /// Structural scan with no allocation: a named error when the input ends
    /// inside a string, quoted identifier, dollar-quote, or block comment. A
    /// migration either splits correctly or fails here — never a silent
    /// truncation. Callers: `applySqlStatements` before apply, and the
    /// migration corpus guard.
    pub fn validate(sql: []const u8) SplitError!void {
        var splitter = SqlStatementSplitter.init(sql);
        while (splitter.next() != null) {}
        if (splitter.in_single_quote) return SplitError.UnterminatedString;
        if (splitter.in_double_quote) return SplitError.UnterminatedQuotedIdentifier;
        if (splitter.dollar_tag != null) return SplitError.UnterminatedDollarQuote;
        if (splitter.unterminated_block_comment) return SplitError.UnterminatedBlockComment;
    }
};
