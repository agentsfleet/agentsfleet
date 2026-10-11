#!/usr/bin/env python3
"""Source rules for the design system (`ui/packages/design-system`).

    python3 scripts/check_design_system_source.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

PACKAGES = "ui/packages"
DS_SRC = f"{PACKAGES}/design-system/src"

# --- Mapped tokens only -------------------------------------------------------
# The components that render through mapped design-system tokens alone. Twelve
# other sites in the package use arbitrary values on purpose (Dialog, Grid,
# Select, Tooltip, DropdownMenu); widening this list is a design decision, not
# a lint change.
TOKEN_ONLY_COMPONENTS = (f"{DS_SRC}/design-system/OptionCard.tsx", f"{DS_SRC}/design-system/UsageBar.tsx")
STRING = re.compile(r""""(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\]|\\.)*`""")
# An arbitrary-VALUE utility (`bg-[#fff]`, `w-[42px]`). A bracket followed by
# `:` is an arbitrary VARIANT (`data-[state=checked]:`), which is standard.
ARBITRARY_VALUE = re.compile(r"(?<![\w-])[a-z][\w-]*-\[[^\]\s]+\](?!:)")
HEX_COLOUR = re.compile(r"#[0-9a-fA-F]{3,8}\b")

# --- Text stays readable on every surface ------------------------------------
TOKENS_CSS = f"{DS_SRC}/tokens.css"
THEMES = {"dark": ":root", "light": '[data-theme="light"]'}
SURFACES = ("bg", "surface-deep", "surface-1", "surface-2", "surface-3")
FOREGROUNDS = ("text", "text-muted", "text-subtle", "pulse", "success", "warn", "error", "info")
MIN_TEXT_CONTRAST = 4.5
CUSTOM_PROPERTY = re.compile(r"--(?P<name>[\w-]+)\s*:\s*(?P<value>[^;]+);")
HEX_VALUE = re.compile(r"^#(?P<digits>[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$")
RGB_VALUE = re.compile(r"^rgb\(\s*(?P<r>\d+)[\s,]+(?P<g>\d+)[\s,]+(?P<b>\d+)\s*\)$")
VAR_VALUE = re.compile(r"^var\(--(?P<name>[\w-]+)\)$")
MAX_CHANNEL = 255
SRGB_THRESHOLD, SRGB_DIVISOR, SRGB_OFFSET, SRGB_SCALE, SRGB_POWER = 0.04045, 12.92, 0.055, 1.055, 2.4
LUMINANCE_WEIGHTS = (0.2126, 0.7152, 0.0722)
CONTRAST_OFFSET = 0.05

# --- One cn ------------------------------------------------------------------
# A second cn drifts from the design system's tailwind-merge configuration.
CN_HOME = f"{DS_SRC}/utils.ts"
CN_DECLARATION = re.compile(r"export (?:function|const) cn\b")


def check_token_only_components(root: Path) -> list[lib.Finding]:
    found = []
    for relative_path in TOKEN_ONLY_COMPONENTS:
        path = root / relative_path
        if not path.is_file():
            found.append(lib.Finding(relative_path, 1, "ds-mapped-tokens", "the component this rule guards is missing"))
            continue
        code = lib.blank_js_comments(path.read_text(encoding="utf-8"))
        for literal in STRING.finditer(code):
            for pattern, what in ((ARBITRARY_VALUE, "an arbitrary-value utility"), (HEX_COLOUR, "a hex colour")):
                for hit in pattern.finditer(literal.group(0)):
                    found.append(lib.Finding(relative_path, lib.line_of(code, literal.start()), "ds-mapped-tokens", f"`{hit.group(0)}` is {what}; use a mapped token"))
    return found


def css_block(css: str, selector: str) -> str | None:
    start = css.find(f"{selector} {{")
    if start == -1:
        return None
    end = css.find("}", start)
    return css[start : end if end != -1 else len(css)]


def resolve(name: str, properties: dict[str, str], seen: frozenset[str] = frozenset()) -> tuple[int, int, int] | None:
    value = properties.get(name, "").strip()
    hex_match, rgb_match, var_match = HEX_VALUE.match(value), RGB_VALUE.match(value), VAR_VALUE.match(value)
    if hex_match:
        digits = hex_match["digits"]
        digits = "".join(c * 2 for c in digits) if len(digits) == 3 else digits
        return tuple(int(digits[i : i + 2], 16) for i in (0, 2, 4))
    if rgb_match:
        return (int(rgb_match["r"]), int(rgb_match["g"]), int(rgb_match["b"]))
    if var_match and var_match["name"] not in seen:
        return resolve(var_match["name"], properties, seen | {name})
    return None


def luminance(rgb: tuple[int, int, int]) -> float:
    def linear(channel: int) -> float:
        value = channel / MAX_CHANNEL
        return value / SRGB_DIVISOR if value <= SRGB_THRESHOLD else ((value + SRGB_OFFSET) / SRGB_SCALE) ** SRGB_POWER
    return sum(linear(c) * w for c, w in zip(rgb, LUMINANCE_WEIGHTS))


def contrast(a: tuple[int, int, int], b: tuple[int, int, int]) -> float:
    la, lb = luminance(a), luminance(b)
    return (max(la, lb) + CONTRAST_OFFSET) / (min(la, lb) + CONTRAST_OFFSET)


def check_contrast(root: Path) -> list[lib.Finding]:
    path = root / TOKENS_CSS
    css = path.read_text(encoding="utf-8") if path.is_file() else ""
    found = []
    for theme, selector in THEMES.items():
        block = css_block(css, selector)
        if block is None:
            found.append(lib.Finding(TOKENS_CSS, 1, "ds-contrast", f"no {selector} block"))
            continue
        line = lib.line_of(css, css.find(f"{selector} {{"))
        properties = {m["name"]: m["value"] for m in CUSTOM_PROPERTY.finditer(block)}
        colours = {name: resolve(name, properties) for name in SURFACES + FOREGROUNDS}
        for name in [n for n, c in colours.items() if c is None]:
            found.append(lib.Finding(TOKENS_CSS, line, "ds-contrast", f"{theme}: --{name} is missing or not a colour this check can evaluate"))
        for fg in FOREGROUNDS:
            for bg in SURFACES:
                if colours[fg] and colours[bg] and contrast(colours[fg], colours[bg]) < MIN_TEXT_CONTRAST:
                    found.append(lib.Finding(TOKENS_CSS, line, "ds-contrast", f"{theme}: --{fg} on --{bg} is {contrast(colours[fg], colours[bg]):.2f}:1, under {MIN_TEXT_CONTRAST}:1"))
    return found


def check_single_cn(root: Path) -> list[lib.Finding]:
    declared = [
        path for path in lib.source_files(root / PACKAGES, (".ts", ".tsx"), include_tests=True)
        if CN_DECLARATION.search(path.read_text(encoding="utf-8"))
    ]
    found = [
        lib.Finding(lib.relative(root, path), 1, "ds-single-cn", f"a second cn; import the one in {CN_HOME}")
        for path in declared if lib.relative(root, path) != CN_HOME
    ]
    if not any(lib.relative(root, path) == CN_HOME for path in declared):
        found.append(lib.Finding(CN_HOME, 1, "ds-single-cn", "the design system's cn is missing"))
    return found


RULES = (check_token_only_components, check_contrast, check_single_cn)


def lint(root: Path) -> list[lib.Finding]:
    return [f for rule in RULES for f in rule(root)]


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "design-system source", lint))
