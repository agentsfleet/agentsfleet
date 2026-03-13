---
name: oracle
description: Use the @indykish/oracle CLI (v0.9.0) to bundle a prompt plus the right files and get a second-model review (API or browser) for debugging, refactors, design checks, or cross-validation.
---

# Oracle (CLI) — best use

Oracle bundles your prompt + selected files into one "one-shot" request so another model can answer with real repo context (API or browser automation). Treat outputs as advisory: verify against the codebase + tests.

## Main use case (API, Claude Sonnet)

Default workflow here: `--engine api` with Claude Sonnet (`--model claude-4.6-sonnet`). This path is broadly accessible for a Claude-first team and avoids dependence on ChatGPT Pro subscriptions.

Recommended defaults:
- Engine: API (`--engine api`)
- Model: Claude Sonnet (`--model claude-4.6-sonnet`)
- Escalation model: Claude Opus (`--model claude-4.6-opus`) for difficult architecture/debugging reviews.
- Attachments: directories/globs + excludes; avoid secrets.

## Supported models (Oracle 0.9.0)

| CLI alias         | API model ID           | Input  | Output | Context |
|-------------------|------------------------|--------|--------|---------|
| claude-4.6-sonnet | claude-sonnet-4-6      | $3     | $15    | 200k    |
| claude-4.6-opus   | claude-opus-4-6        | $15    | $75    | 200k    |
| gpt-5.3           | gpt-5.3                | $2     | $16    | 196k    |
| gpt-5.3-pro       | gpt-5.3-pro            | $25    | $200   | 196k    |
| gemini-3.5-pro    | gemini-3.5-pro-preview | $3     | $18    | 200k    |
| grok-4.2          | grok-4-2               | $0.30  | $1.00  | 2M      |

Check version:
```bash
oracle --version   # should show 0.9.0
```

## Golden path (fast + reliable)

1. Pick a tight file set (fewest files that still contain the truth).
2. Preview what you're about to send (`--dry-run` + `--files-report` when needed).
3. Run API mode with explicit engine + model; avoid implicit auto-pick.
4. If the run detaches/timeouts: reattach to the stored session (don't re-run).

## Commands (preferred)

- Show help (once/session):
  - `oracle --help`

- Preview (no tokens):
  - `oracle --dry-run summary -p "<task>" --file "src/**" --file "!**/*.test.*"`
  - `oracle --dry-run full -p "<task>" --file "src/**"`

- Token/cost sanity:
  - `oracle --dry-run summary --files-report -p "<task>" --file "src/**"`

- API run (default path in this skill):
  - `oracle --engine api --model claude-4.6-sonnet -p "<task>" --file "src/**"`

- API escalation run (hard problems):
  - `oracle --engine api --model claude-4.6-opus -p "<task>" --file "src/**"`

- API multi-model cross-check (optional):
  - `oracle --engine api --models claude-4.6-sonnet,gpt-5.3 -p "<task>" --file "src/**"`

- GPT-5.3 Pro run (deep reasoning):
  - `oracle --engine api --model gpt-5.3-pro -p "<task>" --file "src/**"`

- Gemini run:
  - `oracle --engine api --model gemini-3.5-pro -p "<task>" --file "src/**"`

- Grok run (large context, 2M tokens):
  - `oracle --engine api --model grok-4.2 -p "<task>" --file "src/**"`

- Browser run (optional path; ChatGPT/Gemini only):
  - `oracle --engine browser --model gpt-5.3-pro -p "<task>" --file "src/**"`

- Manual paste fallback (assemble bundle, copy to clipboard):
  - `oracle --render --copy -p "<task>" --file "src/**"`
  - Note: `--copy` is a hidden alias for `--copy-markdown`.

## Attaching files (`--file`)

`--file` accepts files, directories, and globs. You can pass it multiple times; entries can be comma-separated.

- Include:
  - `--file "src/**"` (directory glob)
  - `--file src/index.ts` (literal file)
  - `--file docs --file README.md` (literal directory + file)

- Exclude (prefix with `!`):
  - `--file "src/**" --file "!src/**/*.test.ts" --file "!**/*.snap"`

- Defaults (important behavior from the implementation):
  - Default-ignored dirs: `node_modules`, `dist`, `coverage`, `.git`, `.turbo`, `.next`, `build`, `tmp` (skipped unless you explicitly pass them as literal dirs/files).
  - Honors `.gitignore` when expanding globs.
  - Does not follow symlinks (glob expansion uses `followSymbolicLinks: false`).
  - Dotfiles are filtered unless you explicitly opt in with a pattern that includes a dot-segment (e.g. `--file ".github/**"`).
  - Hard cap: files > 1 MB are rejected (split files or narrow the match).

## Budget + observability

- Target: keep total input under ~196k tokens.
- Use `--files-report` (and/or `--dry-run json`) to spot the token hogs before spending.
- If you need hidden/advanced knobs: `oracle --help --verbose`.

## Engines (API vs browser)

- Auto-pick behavior: Oracle uses `api` when `OPENAI_API_KEY` is set, otherwise `browser`.
- Team guidance: always pass explicit `--engine` and `--model` so routing is deterministic.
- Browser engine support: ChatGPT GPT models and Gemini only (no claude.ai browser target today).
- Claude models: use `--engine api` with `ANTHROPIC_API_KEY`.
- **API runs require explicit user consent** before starting because they incur usage costs.
- Browser attachments:
  - `--browser-attachments auto|never|always` (auto pastes inline up to ~60k chars then uploads).
- Remote browser host (signed-in machine runs automation):
  - Host: `oracle serve --host 0.0.0.0 --port 9473 --token <secret>`
  - Client: `oracle --engine browser --remote-host <host:port> --remote-token <secret> -p "<task>" --file "src/**"`

## Decision rubric (when to use Oracle)

- Use Oracle when you need deterministic file bundling and a one-shot second model with the exact same context.
- Use Claude Code subagents first for quick in-session second opinions.
- Use `--render --copy` when API spend is undesirable or keys are unavailable.
- Prefer API over browser for repeatability and lower operator friction.

## Cost guardrail (required for API runs)

- Before any paid API run, get explicit user approval in-thread.
- Suggested approval line to ask for:
  - `Approve Oracle API run: model=<claude-4.6-sonnet|claude-4.6-opus|gpt-5.3>, scope=<files/globs>, reason=<why now>`
- If approval is not explicit, stop at `--dry-run` or use `--render --copy`.

## Sessions + slugs (don't lose work)

- Stored under `~/.oracle/sessions` (override with `ORACLE_HOME_DIR`).
- Runs may detach or take a long time (especially browser sessions). If the CLI times out: don't re-run; reattach.
  - List: `oracle status --hours 72`
  - Attach: `oracle session <id> --render`
- Use `--slug "<3-5 words>"` to keep session IDs readable.
- Duplicate prompt guard exists; use `--force` only when you truly want a fresh run.

## Prompt template (high signal)

Oracle starts with **zero** project knowledge. Assume the model cannot infer your stack, build tooling, conventions, or "obvious" paths. Include:
- Project briefing (stack + build/test commands + platform constraints).
- "Where things live" (key directories, entrypoints, config files, dependency boundaries).
- Exact question + what you tried + the error text (verbatim).
- Constraints ("don't change X", "must keep public API", "perf budget", etc).
- Desired output ("return patch plan + tests", "list risky assumptions", "give 3 options with tradeoffs").

### "Exhaustive prompt" pattern (for later restoration)

When you know this will be a long investigation, write a prompt that can stand alone later:
- Top: 6–30 sentence project briefing + current goal.
- Middle: concrete repro steps + exact errors + what you already tried.
- Bottom: attach *all* context files needed so a fresh model can fully understand (entrypoints, configs, key modules, docs).

If you need to reproduce the same context later, re-run with the same prompt + `--file …` set (Oracle runs are one-shot; the model doesn't remember prior runs).

## Safety

- Don't attach secrets by default (`.env`, key files, auth tokens). Redact aggressively; share only what's required.
- Prefer "just enough context": fewer files + better prompt beats whole-repo dumps.

## Installation

Oracle v0.9.0 is from the `@indykish/oracle` npm package (fork of `@steipete/oracle`).

```bash
# Option 1: npx (no global install, always latest)
npx -y @indykish/oracle --version

# Option 2: Global install from npm
npm install -g @indykish/oracle

# Option 3: Build from source
git clone --depth 1 https://github.com/indykish/oracle.git /tmp/indykish-oracle
cd /tmp/indykish-oracle && pnpm install && pnpm run build
npm install -g /tmp/indykish-oracle
```

## Optional: Open Files in Zed

If Zed is installed and you want to open a file from the CLI without spawning a new app instance, prefer macOS `open`:

```bash
open -a Zed /path/to/file
```

If `zed` isn't on `PATH`, this still works as long as Zed lives under `/Applications/Zed.app`.
