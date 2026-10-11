"""Where `check_tests_call_code.py` finds code in a test file's text.

Pure text helpers: a Rust `#[cfg(test)]` body, the spans of Python string
literals, the argument a call or command is handed, and the functions a
TypeScript or JavaScript test declares, and JavaScript code with its comments
and literals blanked. None of them knows what a read is.
"""

from __future__ import annotations

from collections.abc import Iterator
import functools
import io
import re
import tokenize

import check_source_lib as lib

# Tokens whose text is a Python literal; an f-string's braced code tokenizes apart.
PYTHON_LITERAL_TOKENS = frozenset({"STRING", "FSTRING_MIDDLE", "TSTRING_MIDDLE"})
# A named function, or a `const` bound to an arrow function with a block body.
JS_FUNCTION = re.compile(
    r"\bfunction\s+(?P<name>[A-Za-z_$][\w$]*)\s*(?:<[^>{]*>)?\s*\((?P<params>[^)]*)\)[^{;]*\{"
    r"|\b(?:const|let|var)\s+(?P<arrow>[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s*)?"
    r"\((?P<arrow_params>[^)]*)\)[^=;{]*=>\s*\{"
)
JS_PARAM = re.compile(r"\s*(?:\.\.\.)?(?P<name>[A-Za-z_$][\w$]*)")
# How much code before a slash, quote or `<` decides whether it opens a literal.
JS_LOOKBEHIND = 64
# Keywords a string may follow; after any other name a quote is JSX text or a misread.
STRING_AFTER_WORDS = lib.REGEX_AFTER_WORDS | {"from", "import", "export", "default"}
# A JSX opening tag or fragment; `<T,>` and `<T extends U>` are type parameters.
JSX_TAG = re.compile(r"<(?:>|[A-Za-z][\w.:-]*(?![\w.:-])(?!\s*(?:,|extends\b)))")


def matching_close(code: str, open_index: int, pair: str = "{}") -> int:
    """The offset of the bracket closing the one at `open_index`, or the end of `code`."""
    depth, index = 0, open_index
    while index < len(code):
        depth += {pair[0]: 1, pair[1]: -1}.get(code[index], 0)
        if depth == 0:
            return index
        index += 1
    return len(code)


class Ambiguous(Exception):
    """Text that may or may not be a literal: from where it opened to the end of `offset`'s line, it stays code."""

    def __init__(self, code: str, offset: int) -> None:
        super().__init__(offset)
        newline = code.find("\n", offset)
        self.end = len(code) if newline == -1 else newline


def blanked(text: str) -> str:
    return "".join(c if c == "\n" else " " for c in text)


def quoted_end(code: str, index: int) -> int:
    """The offset past the quoted string opening at `index`; a line ending first is ambiguous."""
    quote, index = code[index], index + 1
    while index < len(code) and code[index] != "\n":
        if code[index] == quote:
            return index + 1
        index += 2 if code[index] == "\\" else 1
    raise Ambiguous(code, index)


def string_may_start(before: str) -> bool:
    """Whether a quote after `before` can open a string: never straight after a name or a closer."""
    before = before.rstrip()
    if not before or not (before[-1].isalnum() or before[-1] in "_$)]"):
        return True
    word = lib.IDENTIFIER_TAIL.search(before)
    return bool(word) and word.group(0) in STRING_AFTER_WORDS


def comment_end(code: str, index: int) -> int:
    if code.startswith("//", index):
        newline = code.find("\n", index)
        return len(code) if newline == -1 else newline
    close = code.find("*/", index + 2)
    if close == -1:
        raise Ambiguous(code, len(code))
    return close + 2


def masked_literal(code: str, index: int) -> tuple[str, int] | None:
    """The comment or literal opening at `index` masked, and the offset past it; None when none opens."""
    char, before = code[index], code[max(0, index - JS_LOOKBEHIND) : index]
    if code.startswith(("//", "/*"), index):
        end = comment_end(code, index)
    elif char == "`":
        return masked_template(code, index)
    elif char in "\"'" and string_may_start(before):
        end = quoted_end(code, index)
    elif char == "/" and lib._regex_may_start(before):
        end = lib._regex_end(code, index)  # no closing slash on the line: a division
    elif char == "<" and lib._regex_may_start(before) and JSX_TAG.match(code, index):
        return masked_jsx(code, index)
    else:
        return None
    return None if end is None else (blanked(code[index:end]), end)


def masked_code(code: str, index: int, nested: bool) -> tuple[str, int]:
    """Code from `index` with literals blanked, to the end or, when `nested`, past the unmatched `}`."""
    out, depth = [], 0
    while index < len(code):
        char = code[index]
        if nested and char == "}" and depth == 0:
            return "".join(out) + " ", index + 1
        try:
            literal = masked_literal(code, index)
        except Ambiguous as ambiguous:
            if nested:
                raise
            literal = code[index : ambiguous.end], ambiguous.end
        if literal is not None:
            out.append(literal[0])
            index = literal[1]
            continue
        depth += {"{": 1, "}": -1}.get(char, 0)
        out.append(char)
        index += 1
    if nested:
        raise Ambiguous(code, len(code))
    return "".join(out), index


def masked_template(code: str, index: int) -> tuple[str, int]:
    """The template literal opening at `index` with its text blanked and each `${…}` kept as code."""
    out, index = [" "], index + 1
    while index < len(code):
        if code[index] == "`":
            return "".join(out) + " ", index + 1
        if code.startswith("${", index):
            expression, index = masked_code(code, index + 2, nested=True)
            out.append("  " + expression)
            continue
        step = 2 if code[index] == "\\" else 1
        out.append(blanked(code[index : index + step]))
        index += step
    raise Ambiguous(code, len(code))


def masked_jsx_part(code: str, index: int, children: bool) -> tuple[str, int]:
    """A JSX tag (attribute strings blanked) or child (text kept): each `{…}` masked as code."""
    char = code[index]
    if char == "{":
        expression, end = masked_code(code, index + 1, nested=True)
        return " " + expression, end
    if children and char == "<":
        return masked_jsx(code, index)
    if not children and char in "\"'":
        end = quoted_end(code, index)
        return blanked(code[index:end]), end
    if char in ("}>" if children else "<};"):
        raise Ambiguous(code, index)  # never legal here: this may never have been JSX
    return char, index + 1


def masked_tag(code: str, index: int) -> tuple[str, int]:
    out, index = ["<"], index + 1
    while index < len(code):
        if code[index] == ">":
            return "".join(out) + ">", index + 1
        text, index = masked_jsx_part(code, index, children=False)
        out.append(text)
    raise Ambiguous(code, len(code))


def masked_jsx(code: str, index: int) -> tuple[str, int]:
    """The JSX element opening at `index`; text that fails as JSX is ambiguous."""
    tag, index = masked_tag(code, index)
    if tag.endswith("/>"):
        return tag, index
    out = [tag]
    while index < len(code):
        if code.startswith("</", index):
            close, index = masked_tag(code, index)
            return "".join(out) + close, index
        text, index = masked_jsx_part(code, index, children=True)
        out.append(text)
    raise Ambiguous(code, len(code))


@functools.lru_cache(maxsize=16)
def js_code_only(code: str) -> str:
    """`code` at the same length with comments and literal text blanked to spaces, newlines kept.

    Fail-closed: text is blanked only when it is certainly a comment or a
    literal. A quote straight after a name opens nothing, and an unterminated
    string, template or comment, or text that fails as JSX, stays code to the
    end of the line where it failed. A template's `${…}` and JSX `{…}` are code;
    JSX child text is kept, so its quotes open nothing.
    """
    return masked_code(code, 0, nested=False)[0]


def in_js_literal(code: str, offset: int, language: str) -> bool:
    """True when the JavaScript character at `offset` is certainly comment or literal text."""
    return language == "js" and js_code_only(code)[offset] != code[offset]


def cfg_test_regions(source: str) -> list[tuple[int, int]]:
    """Offsets of each `#[cfg(test)]` item's body in a Rust source file."""
    regions = []
    for match in re.finditer(r"#\[cfg\(test\)\]", source):
        brace, semicolon = source.find("{", match.end()), source.find(";", match.end())
        if brace == -1 or (semicolon != -1 and semicolon < brace):
            continue  # `mod tests;` — the body is its own file
        regions.append((match.start(), matching_close(source, brace)))
    return regions


def python_literal_spans(code: str) -> list[tuple[int, int]]:
    """Offsets of each Python string literal's text, from the standard tokenizer."""
    line_offsets = [0]
    for line in code.splitlines(keepends=True):
        line_offsets.append(line_offsets[-1] + len(line))
    spans = []
    try:
        for token in tokenize.generate_tokens(io.StringIO(code).readline):
            if tokenize.tok_name[token.type] in PYTHON_LITERAL_TOKENS:
                (start_row, start_col), (end_row, end_col) = token.start, token.end
                spans.append((line_offsets[start_row - 1] + start_col, line_offsets[end_row - 1] + end_col))
    except (tokenize.TokenError, SyntaxError):
        pass  # source that will not tokenize keeps the literals found so far, and every read is a candidate
    return spans


def call_argument(code: str, start: int, language: str) -> str:
    """The text a read call is handed: its parenthesised arguments, or the rest of the command."""
    line_end = code.find("\n", start)
    line_end = len(code) if line_end == -1 else line_end
    if language == "shell":
        while code[:line_end].rstrip().endswith("\\") and line_end < len(code):
            next_end = code.find("\n", line_end + 1)
            line_end = len(code) if next_end == -1 else next_end
        return code[start:line_end]
    paren = code.find("(", start, line_end + 1)
    if paren == -1:
        return code[start:line_end]
    return code[paren + 1 : matching_close(code, paren, "()")]


def js_functions(code: str) -> Iterator[tuple[str, set[str], str, int]]:
    """Each declared function's name, parameter names, body text and declaration offset."""
    for match in JS_FUNCTION.finditer(code):
        name = match["name"] or match["arrow"]
        params_text = match["params"] if match["name"] else match["arrow_params"]
        params = {m["name"] for part in params_text.split(",") if (m := JS_PARAM.match(part))}
        body_start = match.end() - 1
        yield name, params, code[body_start : matching_close(js_code_only(code), body_start)], match.start()
