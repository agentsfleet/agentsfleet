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
# How much code before a slash decides whether it opens a pattern literal.
JS_LOOKBEHIND = 64


def matching_close(code: str, open_index: int, pair: str = "{}") -> int:
    """The offset of the bracket closing the one at `open_index`, or the end of `code`."""
    depth, index = 0, open_index
    while index < len(code):
        depth += {pair[0]: 1, pair[1]: -1}.get(code[index], 0)
        if depth == 0:
            return index
        index += 1
    return len(code)


def js_literal_end(code: str, index: int) -> int | None:
    """The offset past the string, template or pattern literal opening at `index`, or None."""
    if code[index] in "\"'`":
        return lib._quoted_end(code, index)
    if code[index] == "/" and lib._regex_may_start(code[max(0, index - JS_LOOKBEHIND) : index]):
        return lib._regex_end(code, index)
    return None


@functools.lru_cache(maxsize=16)
def js_code_only(code: str) -> str:
    """`code` at the same length with comments and literals blanked to spaces, newlines kept.

    Offsets stay valid, and no text inside a string, template, pattern or
    comment can open or close a bracket or spell a call.
    """
    code = lib.blank_js_comments(code)
    out, index = [], 0
    while index < len(code):
        end = js_literal_end(code, index)
        if end is None:
            out.append(code[index])
            index += 1
            continue
        out.append("".join(c if c == "\n" else " " for c in code[index:end]))
        index = end
    return "".join(out)


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
