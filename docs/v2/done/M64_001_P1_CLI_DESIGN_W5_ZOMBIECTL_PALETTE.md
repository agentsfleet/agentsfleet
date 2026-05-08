<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M64_001: zombiectl renders the Operational Restraint design system in 256-color terminals

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 001
**Date:** May 08, 2026
**Status:** DONE
**Priority:** P1 — the CLI is the highest-frequency, lowest-stakes brand surface; engineers see it 100x more than the marketing site and it must read consistent with the web brand.
**Categories:** CLI
**Batch:** B1
**Branch:** feat/m64-001-design-w5
**Depends on:** `docs/DESIGN_SYSTEM.md` v0.1 (merged via PR #306) — defines the palette, glyphs, and the "no decorative ASCII art" rule this spec implements.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` "CLI / zombiectl rendering" section — the canonical 256-color palette mapping table and the status-glyph rules.

---

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` "CLI / zombiectl rendering" section + every Forbidden-color/motion list — the canonical palette, glyph, and currency rules. The `--pulse` token is currency: every additional use dilutes it.
2. `docs/BUN_RULES.md` — `*.js` discipline (this CLI is plain JS not TS, but BUN_RULES applies). Particular attention: §2 const discipline, §10 anti-patterns. **§8 forbids snapshot tests by default**; the spec carves out *golden text fixtures* (byte-exact `.txt` files under `test/golden/`) as the only acceptable form for CLI output assertions — see Discovery.
3. `docs/LOGGING_STANDARD.md` §1 — confirms zombiectl's `writeLine(stdout, ...)` is human-rendering, not collector-bound log emit. Style helpers carry no `level=`/`scope=`/`event=` fields. The LOGGING GATE is satisfied by this carve-out (recorded in Discovery).
4. `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html` Mockup C — the North Star for `zombiectl list` and `zombiectl steer` rendering. Source of truth when the palette mapping leaves room for interpretation (e.g., the `LIVE` text label gets evidence-amber, not pulse-cyan; the `DIAGNOSIS` label gets success-green).
5. `zombiectl/src/ui-theme.js` (current) and `zombiectl/src/program/banner.js` (current) — the modules being replaced. Banner currently emits a 🧟 emoji + box-drawing border around the version; both go.
6. `zombiectl/test/banner.unit.test.js` and `zombiectl/test/helpers.js` — the existing test patterns (`bun:test`, `makeBufferStream`, `stripAnsi` helper). Mirror these.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal floor; specifically RULE UFS (string-literal extraction; ANSI escape codes named once), RULE FLL (file/function length), RULE TST-NAM (no milestone IDs in test names or source bodies), RULE ORP (orphan sweep — every retired symbol grepped to zero across `src/` + `test/`).
- `docs/BUN_RULES.md` — §2 const, §3 imports, §7 naming, §10 anti-patterns. §8 carves out golden text fixtures (see Discovery).
- `docs/DESIGN_SYSTEM.md` — palette currency, status glyph table, no decorative ASCII art.
- `docs/LOGGING_STANDARD.md` §1 — scope carve-out for human-rendering helpers (Discovery records).

---

## Overview

**Goal (testable):** `zombiectl` renders every command output through one centralized style module that mirrors the Operational Restraint web palette in 256-color terminals; `--pulse` cyan appears only on live-state glyphs, the `--version` brand mark, and help-text section headings; status glyphs are deterministic (`●` live, `○` parked, `●` warn, `✕` failed); `NO_COLOR` / non-TTY / `--json` modes emit zero ANSI escape sequences; help output stays ≤80 columns wide.

**Problem:** The CLI's current styling drifts from the design system in three ways: (1) `ui-theme.js` uses a 256-color orange (`38;5;208`) for headings and info — a brand color that doesn't exist in `docs/DESIGN_SYSTEM.md`; (2) the `--version` banner prints a 🧟 zombie emoji + box-drawing border, both forbidden as decorative ASCII art; (3) styling escapes are scattered (banner.js inlines its own `[...]m` builders, ui-progress.js draws its own success/fail glyphs). An engineer running `zombiectl list` next to `app.usezombie.com` sees two unrelated products.

**Solution summary:** Replace `src/ui-theme.js` and `src/ui-progress.js` with a single style package under `src/output/` that exposes `palette`, `glyph`, `formatTable`, `formatHelpHeading`, `formatEvidence`, and a TTY-capability detector. Every command consumes the package; no module assembles ANSI escapes inline. The `--version` banner becomes a one-line print of `zombiectl v<version>` with a single pulse-cyan dot prefix — no emoji, no border. `NO_COLOR` / non-TTY / `--json` paths route through the same helpers and emit plain ASCII. Existing command semantics are unchanged; this spec is purely the rendering layer.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/output/palette.js` | CREATE | The single source of 256-color codes mapped to design-system tokens. One file, no scattered escapes. |
| `zombiectl/src/output/glyph.js` | CREATE | Named exports `live()`, `parked()`, `degraded()`, `failed()`, `ok()`, `error()` — glyph + color paired by name; commands never hard-code the pair. |
| `zombiectl/src/output/format.js` | CREATE | `formatTable`, `formatKeyValue`, `formatSection`, `formatHelpHeading`, `formatEvidence`. Width-aware (reads `process.stdout.columns`). |
| `zombiectl/src/output/capability.js` | CREATE | `detectColorMode(env, stream)` returns `none` / `basic16` / `xterm256`. Honours `NO_COLOR`, `FORCE_COLOR`, `process.stdout.isTTY`, `--json`, terminal capability env (`TERM`/`COLORTERM`). |
| `zombiectl/src/output/index.js` | CREATE | Public entry point — re-exports the helpers commands consume. |
| `zombiectl/src/ui-theme.js` | DELETE | Replaced by `src/output/`. Orange 256:208 retired. |
| `zombiectl/src/ui-progress.js` | EDIT | Spinner stays (no replacement needed); rewires its `\r✔`/`\r✖` finalizers through `glyph.ok()`/`glyph.error()`. Disables when `!isTTY` (was already gated, now goes through capability detector). |
| `zombiectl/src/program/banner.js` | EDIT | Strip the 🧟 emoji + box-drawing border. Reduce to one-line `<pulse-dot> zombiectl v<version>`. Pre-release warning keeps its single `⚠` glyph (functional, not decorative). |
| `zombiectl/src/program/io.js` | EDIT | `printHelp` consumes `formatHelpHeading` for `USAGE:`/`OPTIONS:`/`COMMANDS:` blocks; help text wraps at ≤80 columns. |
| `zombiectl/src/cli.js` | EDIT | Swap `import { ui } from "./ui-theme.js"` for `import { ui } from "./output/index.js"`; keep the `ui.ok/info/warn/err/dim/head` shape so command modules don't churn. |
| `zombiectl/src/commands/*.js` | EDIT | Audit only — no behaviour changes. Rewire any inline glyph hardcodes (e.g. `🎉` in `commands/zombie.js:127`) to `glyph.ok()` + plain text. |
| `zombiectl/test/output-palette.unit.test.js` | CREATE | Palette token → ANSI mapping; capability detection (NO_COLOR, !isTTY, JSON, basic16 fallback). |
| `zombiectl/test/output-glyph.unit.test.js` | CREATE | Each glyph function returns the correct char + ANSI sequence; matches Mockup C exactly. |
| `zombiectl/test/output-format.unit.test.js` | CREATE | Table widths at 80/120/40 columns; right-align numerics, left-align text; help-heading colour; evidence-line shape. |
| `zombiectl/test/banner.unit.test.js` | EDIT | Update assertions: no 🧟 emoji, no box-drawing chars, version line ≤80 columns. |
| `zombiectl/test/help.test.js` | EDIT | Pin help output ≤80 columns wide; no emoji; section headings carry the pulse-cyan ANSI sequence in TTY mode. |
| `zombiectl/test/golden/*.txt` | CREATE | Byte-exact text fixtures: `zombiectl list` (TTY + NO_COLOR), `zombiectl --help`, `zombiectl --version`, `zombiectl steer` evidence rendering. Diffed against captured output in CI. |
| `~/Projects/docs/changelog.mdx` | EDIT | Add `<Update>` entry under v2 unreleased — user-visible CLI brand alignment. |

---

## Sections (implementation slices)

### §1 — Centralized output package

Create `src/output/` with `palette.js`, `glyph.js`, `format.js`, `capability.js`, `index.js`. The package replaces `src/ui-theme.js` outright; no compat shim (RULE NLG, pre-v2.0.0). The public surface mirrors the existing `ui` shape (`ok/info/warn/err/dim/head`) so command modules consume the new package by import-rename only.

**Implementation default:** the `ui` proxy stays inside `output/index.js` and resolves color/glyph at call time (not module load time) so tests can override `process.stdout.isTTY` and `process.env.NO_COLOR` per test. The current `ui-theme.js` evaluates `useColor` once at import time, which makes per-test capability switching impossible.

### §2 — Capability detection and color mode

`capability.detectColorMode(env, stream)` returns one of `none` / `basic16` / `xterm256`. Decision order:

1. If `--json` is on the command, return `none` (commands signal this through `ctx.jsonMode`; helpers honour the flag without separate plumbing).
2. If `process.env.NO_COLOR` is set to any non-empty value, return `none` (no-color.org spec).
3. If `stream.isTTY !== true`, return `none`.
4. If `process.env.FORCE_COLOR === "0"`, return `none`; if `"1"`/`"2"`/`"3"`, return basic16/xterm256/xterm256.
5. Else read `process.env.TERM` / `process.env.COLORTERM`; xterm256 when terminal advertises 256+, else basic16.

When the detector returns `basic16`, the palette layer maps each xterm256 token to its closest 16-color basic ANSI code (cyan/yellow/green/red, etc.). The detector emits one stderr warning the first call per process, then is silent — engineers on legacy terminals see one notice, not a stream.

### §3 — Status glyphs and the pulse currency rule

`glyph.live()`/`glyph.parked()`/`glyph.degraded()`/`glyph.failed()` return the (char, ANSI) pair per `docs/DESIGN_SYSTEM.md`:

- Live → `●` in pulse-cyan
- Parked → `○` in subtle-grey
- Degraded → `●` in warn-amber
- Failed → `✕` in error-red

**Pulse-cyan currency rule** (audit during VERIFY): `palette.pulse(...)` may be called from these sites only — `glyph.live()`, `banner.printVersion()`, `formatHelpHeading()`. The interactive prompt indicator is not in scope (no REPL today). Anywhere else is a violation; CHORE(close) greps the diff and rejects extras.

### §4 — Banner and help

The `--version` banner reduces to a single line: `<pulse-dot> zombiectl v<version>` followed by an optional dim commit-sha line. No emoji, no border, no autonomous-agent-cli subtitle. The pre-release warning keeps its `⚠` glyph (functional warn-amber) and remains a one-liner.

`printHelp` rewires section headings (`USAGE:`/`OPTIONS:`/`COMMANDS:`/`GLOBAL FLAGS:`/`ENVIRONMENT VARIABLES:`) through `formatHelpHeading()` (pulse-cyan, uppercase). Flag descriptions render through `palette.muted()`. Help output wraps at ≤80 columns; long descriptions wrap or move to a follow-up line, never relying on terminal width.

### §5 — Table rendering and width awareness

`formatTable(columns, rows, opts)` accepts a `widthHint` option (defaults to `process.stdout.columns ?? 100`). Numeric columns right-align; text columns left-align. When width drops below 80 columns, the renderer collapses to a vertical key:value layout (one row block per record). The existing `printTable` ANSI signature is preserved so command modules consume the new helper as a drop-in.

### §6 — Decorative-ASCII teardown

Audit and remove every decorative emoji + ASCII art: the 🧟 in `banner.js`, the 🎉 in `commands/zombie.js:127`, the box-drawing border around the version. Status glyphs (●○✕) and functional indicators (⚠ for pre-release, ⠋⠙⠹⠸ for the spinner) are NOT decorative and stay. The diff is greppable: `git diff -U0 origin/main | rg '\\p{Emoji_Presentation}'` returns zero from `src/`.

### §7 — Golden text fixtures

`zombiectl/test/golden/<command>-<mode>.txt` captures byte-exact output for the command surfaces touched by this spec: `list-tty.txt`, `list-no-color.txt`, `help-tty.txt`, `help-no-color.txt`, `version-tty.txt`, `version-no-color.txt`, `steer-evidence-tty.txt`. Tests compare captured output to the fixture; mismatches fail loud with a unified diff.

**Carve-out:** `docs/BUN_RULES.md` §8 forbids snapshot tests by default. Golden text fixtures are the carve-out: the assertion *is* the byte-exact rendering (CLI contracts are surface-area), they're checked in as plain `.txt` (reviewable in PR), and updates are deliberate (not auto-snapshot regeneration). The Discovery section records this consult.

---

## Interfaces

### `src/output/index.js` public surface

```
ui = {
  ok(message)    -> string  // glyph.ok() + " " + message
  info(message)  -> string  // muted, no glyph (legacy info paths)
  warn(message)  -> string  // glyph.warn-glyph + warn-amber
  err(message)   -> string  // glyph.failed() + error-red
  dim(text)      -> string  // subtle-grey
  head(text)     -> string  // formatHelpHeading proxy (pulse-cyan, upper)
}

palette = { pulse(s), evidence(s), success(s), warn(s), error(s), muted(s), subtle(s), text(s) }

glyph = { live(), parked(), degraded(), failed(), ok(), error(), warn() }
  // each returns { char, color, render() } so callers can swap separator without re-pairing color.

format = {
  table(columns, rows, opts?)   -> string
  keyValue(rows)                -> string
  section(title)                -> string
  helpHeading(title)            -> string
  evidence(ref, quote)          -> string  // "EVIDENCE <ref> — \"<quote>\"" with token coloring
}

capability = {
  detectColorMode(env, stream)  -> "none" | "basic16" | "xterm256"
  isTty(stream)                 -> boolean
}
```

### `src/program/banner.js`

```
printVersion(stream, version, opts?)  // replaces printBanner — single-line, no emoji, no box
printPreReleaseWarning(stream, opts?) // unchanged shape; rewired through palette.warn
```

`printBanner` is removed. Callers (`cli.js`, `program/io.js`) update to `printVersion`.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `process.stdout.columns` undefined | Output piped to `wc -l`, log file, or other non-tty consumer. | `format.table` defaults to `widthHint=100`; capability detector independently returns `none` because `!isTTY`, so no ANSI escapes ship anyway. |
| `NO_COLOR=` (empty string) | Empty env var. | Per no-color.org, *any non-empty value* disables color; an empty string is treated as unset (matches existing `ui-theme.js` behaviour). |
| `TERM=dumb` | Legacy terminal (`emacs -nw -batch`, `script(1)` recordings). | Capability detector returns `none`. No warning emitted. |
| 16-color terminal | `TERM=xterm-color` (no 256). | Capability detector returns `basic16`. One stderr notice per process: `note: terminal advertises <256 colors; using basic palette`. |
| `--json` requested in TTY | Engineer pipes JSON output to `jq`. | `ctx.jsonMode` short-circuits all helpers to plain text; no escape sequences in JSON. |
| Width below 40 columns | Phone tether, narrow tmux pane. | `format.table` collapses to vertical key:value. `printHelp` wraps long lines at the buffer edge. |
| Spinner left running on crash | Unhandled rejection mid-spinner. | Existing `try/finally` in `withSpinner` already calls `.fail()`; carry forward. Disables entirely when `!isTTY`. |

---

## Invariants

1. `palette.pulse(...)` is called from exactly three modules: `output/glyph.js` (live glyph), `program/banner.js` (version dot), `output/format.js` (helpHeading). Enforced by VERIFY-time grep: `rg "palette\\.pulse" src/` returns ≤3 file matches.
2. No file under `src/` calls `[` directly except `output/palette.js`. Enforced by VERIFY-time grep: `rg '\\\\u001b\\[' src/` matches only `palette.js`. (RULE UFS — string-literal extraction.)
3. `NO_COLOR=1 zombiectl <any-command> | rg -P '\\x1b\\['` returns zero matches. Enforced by golden fixtures + a dedicated unit test.
4. No file in scope exceeds 350 lines; no function exceeds 50 lines. Enforced by File & Function Length Gate.
5. No milestone IDs (`M64_001`, `§N.M`, `T7`, `dim N.M`) appear in any source body or test name. Enforced by RULE TST-NAM and the combined HARNESS VERIFY audit.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `palette_xterm256_pulse_emits_color_79` | `palette.pulse("hi")` in xterm256 mode returns `"[38;5;79mhi[0m"`. |
| `palette_no_color_returns_plain` | `NO_COLOR=1` → `palette.pulse("hi") === "hi"` byte-exact. |
| `palette_basic16_falls_back_to_cyan` | Basic16 mode → pulse uses ANSI `36` (basic cyan), evidence uses `33` (yellow), error uses `31`, success uses `32`, warn uses `33`. |
| `capability_pipe_returns_none` | `stream.isTTY = false` → `detectColorMode` returns `none` regardless of TERM. |
| `capability_force_color_overrides_isTty` | `FORCE_COLOR=2` + `!isTTY` → returns `xterm256`. |
| `capability_warning_emits_once` | First call in basic16 mode writes one notice to stderr; subsequent calls are silent. |
| `glyph_live_renders_pulse_dot` | `glyph.live().render()` equals `"[38;5;79m●[0m"`. |
| `glyph_parked_renders_subtle_circle` | `glyph.parked().render()` equals `"[38;5;240m○[0m"`. |
| `glyph_degraded_warn_dot` | Degraded uses `●` in 256:214 — distinct from live. |
| `glyph_failed_render_error_x` | Failed renders `✕` in 256:210. |
| `format_table_right_aligns_numerics` | A column whose every value parses as a finite Number right-aligns. |
| `format_table_collapses_below_80_cols` | `widthHint=40` switches to vertical key:value layout. |
| `format_help_heading_pulse_in_tty` | `formatHelpHeading("USAGE:")` in TTY mode begins with the pulse-cyan ANSI sequence. |
| `format_help_heading_plain_in_no_color` | Same call under `NO_COLOR=1` returns `"USAGE:"` byte-exact. |
| `format_evidence_three_token_split` | `formatEvidence("cd_logs:281–294", "npm ERR! ENOSPC...")` renders `EVIDENCE` in evidence-amber, ref in default text, `— "<quote>"` in muted-grey. |
| `banner_no_emoji_or_box` | `printVersion(stream, "0.33.1", { noColor: true })` output contains no emoji and no `│`/`╰`/`╮`/etc box-drawing chars. |
| `help_output_under_80_cols` | Every line of `printHelp(stdout, ui, { ... })` output is ≤80 columns wide. |
| `golden_list_tty` | Captured `zombiectl list` output (mocked workspace + zombies) byte-exact equals `test/golden/list-tty.txt`. |
| `golden_list_no_color` | Same with `NO_COLOR=1` byte-exact equals `test/golden/list-no-color.txt`. |
| `golden_help_no_emoji` | `test/golden/help-no-color.txt` contains no emoji codepoints. |
| `pulse_currency_audit` | `rg "palette\\.pulse" zombiectl/src/` matches at most three files (regression guard for the currency rule). |
| `no_inline_ansi_audit` | `rg '\\\\u001b\\[' zombiectl/src/` outside `output/palette.js` is empty (regression guard for RULE UFS). |

Negative tests cover every Failure Mode row above; edge tests cover empty TERM, NO_COLOR with whitespace value, and width=0.

---

## Acceptance Criteria

- [ ] `cd zombiectl && bun run test` passes — verify: `bun run test` in `zombiectl/`.
- [ ] `make lint` clean — verify: `make lint`.
- [ ] No file in scope exceeds 350 lines — verify: `git diff --name-only origin/main -- 'zombiectl/src/**' 'zombiectl/test/**' | grep -v '\.md$' | xargs wc -l | awk '$1 > 350 { print "OVER:", $0 }'`.
- [ ] No function exceeds 50 lines — verify: review during VERIFY; LENGTH GATE per edit.
- [ ] `NO_COLOR=1 zombiectl list 2>&1 | rg -P '\x1b\[' | wc -l` returns 0 — verify: run after building, against a mocked API or with the integration harness.
- [ ] `zombiectl list | cat | rg -P '\x1b\['` returns zero matches — verify: piped output is plain ASCII.
- [ ] `zombiectl --help | awk '{ if (length($0) > 80) print "OVER:", length($0), $0 }' | wc -l` returns 0 — verify: help output ≤80 columns.
- [ ] `zombiectl --version` output contains no emoji and no box-drawing characters — verify: `zombiectl --version | rg -P '[\x{1F300}-\x{1FAFF}]|[\x{2500}-\x{257F}]'` empty.
- [ ] `gitleaks detect` clean — verify: `gitleaks detect`.
- [ ] Pulse-cyan currency audit passes — verify: `rg "palette\.pulse" zombiectl/src/ | cut -d: -f1 | sort -u | wc -l` returns ≤3.
- [ ] No inline ANSI escapes outside `palette.js` — verify: `rg '\\u001b\[' zombiectl/src/ | rg -v 'output/palette.js' | wc -l` returns 0.
- [ ] Version sync clean — verify: `make check-version`. (No `VERSION` bump in this spec; `make sync-version` only invoked if a touched file forces it.)
- [ ] Cross-terminal taste check — manual verification on macOS Terminal.app, iTerm2, Alacritty, tmux. Documented in PR Session Notes.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: unit tests (zombiectl directory)
cd zombiectl && bun run test 2>&1 | tail -10

# E2: lint
make lint 2>&1 | tail -10

# E3: NO_COLOR audit on rendered output (uses tests' golden capture)
NO_COLOR=1 node zombiectl/bin/zombiectl.js --help | rg -P '\x1b\[' | wc -l

# E4: help output width
node zombiectl/bin/zombiectl.js --help | awk '{ if (length($0) > 80) print "OVER:", length($0), $0 }' | wc -l

# E5: version banner — no emoji, no box
node zombiectl/bin/zombiectl.js --version | rg -P '[\x{1F300}-\x{1FAFF}]|[\x{2500}-\x{257F}]' | wc -l

# E6: pulse currency
rg "palette\.pulse" zombiectl/src/ | cut -d: -f1 | sort -u

# E7: inline ANSI escapes outside palette
rg '\\u001b\[' zombiectl/src/ | rg -v 'output/palette.js'

# E8: orphan sweep — old ui-theme.js retired
rg "from \"\.\./ui-theme\"|from \"\./ui-theme\"" zombiectl/src/ zombiectl/test/

# E9: gitleaks
gitleaks detect 2>&1 | tail -3

# E10: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | grep -E 'zombiectl/' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER:", $0 }'
```

---

## Dead Code Sweep

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|----------------|----------------|
| `zombiectl/src/ui-theme.js` | `test ! -f zombiectl/src/ui-theme.js` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| `from "./ui-theme.js"` / `from "../ui-theme.js"` | `rg 'from ".*ui-theme' zombiectl/` | 0 matches |
| `printBanner` (replaced by `printVersion`) | `rg '\bprintBanner\b' zombiectl/` | 0 matches |
| 256:208 orange escape (`38;5;208`) | `rg '38;5;208' zombiectl/` | 0 matches |
| `🧟` zombie emoji | `rg '🧟' zombiectl/` | 0 matches |
| `🎉` party emoji in `commands/zombie.js` | `rg '🎉' zombiectl/` | 0 matches |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against the Test Specification table. | Skill returns clean. Iteration count + final coverage summary in PR Session Notes. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec, `docs/DESIGN_SYSTEM.md`, `docs/BUN_RULES.md`, Failure Modes, Invariants. | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` opens the PR | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before requesting human review. |
| After every push | `kishore-babysit-prs` | Polls greptile per cadence; triages P0/P1 vs RULES.md; fixes + replies + reschedules; stops on two consecutive empty polls. | Final report in PR Session Notes. |

---

## Discovery (consult log)

> Filled in during PLAN/EXECUTE as Legacy-Design / Architecture / Doc-Read consults fire.

- **LOGGING GATE carve-out (PLAN, May 08, 2026):** zombiectl's `writeLine(stdout, ...)` rendering is *not* a structured-log emit per `docs/LOGGING_STANDARD.md` §1 (which lists "TypeScript console.log" as in-scope but specifies logfmt with `level=`/`scope=`/`event=` keys). The CLI's helpers carry no log-record metadata; they are human-facing UI rendering. The gate is satisfied by emitting no new collector-bound log lines from this diff. Recorded for VERIFY.
- **BUN_RULES §8 snapshot-test carve-out (PLAN, May 08, 2026):** `docs/BUN_RULES.md` §8 forbids snapshot tests by default. Golden text fixtures are the only acceptable assertion form for byte-exact CLI rendering — they're checked in as plain `.txt`, diff cleanly in PR, and updates are deliberate (no auto-snapshot regen). The spec ships them under `zombiectl/test/golden/` per §7.

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `cd zombiectl && bun run test` | _pending_ | |
| Lint | `make lint` | _pending_ | |
| Help width | E4 above | _pending_ | |
| NO_COLOR audit | E3 above | _pending_ | |
| Version banner | E5 above | _pending_ | |
| Pulse currency | E6 above | _pending_ | |
| Inline ANSI sweep | E7 above | _pending_ | |
| Gitleaks | E9 above | _pending_ | |
| 350L gate | E10 above | _pending_ | |
| Orphan sweep | Dead Code Sweep above | _pending_ | |

---

## Out of Scope

- Web surfaces (`ui/packages/website`, `ui/packages/app`, `ui/packages/design-system`, `docs.usezombie.com`) — handled by W1–W4.
- Backend, schema, API behaviour.
- The actual SKILL.md / TRIGGER.md installation logic — only the *output rendering* of the install command.
- zombiectl command semantics: argument parsing, exit codes, business logic. This spec is purely the visual layer; data correctness is not in scope. If `zombiectl list` returned wrong data before, it returns wrong data after — file a separate spec.
- Interactive REPL prompt (no REPL exists today). The pulse-cyan reservation lists "interactive prompt indicator if zombiectl has REPL mode" — a reservation only; no glyph ships in this spec.
- Berkeley Mono / commercial font upgrade — fonts don't apply to terminal rendering.
- True-color (24-bit) rendering — defer until a follow-up spec; the design system specifies the 256-color mapping as canonical.
