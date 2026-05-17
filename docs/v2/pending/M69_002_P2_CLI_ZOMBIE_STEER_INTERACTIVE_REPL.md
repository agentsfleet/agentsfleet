# M69_002: `zombiectl steer <zombie_id>` enters multi-turn REPL when stdin is a TTY

**Prototype:** v2.0.0
**Milestone:** M69
**Workstream:** 002
**Date:** May 14, 2026
**Status:** PENDING
**Priority:** P2 — terminal-ergonomic improvement on top of an already-working single-shot path; not a correctness fix.
**Categories:** CLI
**Batch:** B1 — independent of other M69 workstreams.
**Branch:** {feat/m69-steer-repl — added when work begins}
**Depends on:** M23_001 (existing `zombie steer` endpoint + SSE response), M63_004 (CLI obs/resilience patterns).
**Provenance:** LLM-drafted (claude-opus-4-7, 2026-05-14) from Captain Q&A scoping 2026-05-14 (decision: "detect human at terminal and go to repl or else it stay for an agent like said no repl needed for an agent. If --tty or some flag then the repl can be forced.").

**Canonical architecture:** `docs/architecture/user_flow.md` §8.2 (`zombiectl` as the local operator surface).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/zombie_steer.js` — current single-shot implementation. Interactive REPL stub already present (`MISSING_ARGUMENT` "interactive steer is not yet implemented" at line ~42). New REPL mode replaces that stub.
2. `zombiectl/src/program/cli-tree.js:323` — command surface is `zombiectl steer <zombie_id> <message>` (flat, NOT nested under `zombie`). Argument shape stays; `<message>` becomes optional when entering REPL mode.
3. `zombiectl/src/output/capability.js` — exports `isTty(stream)` from M64_001. **Reuse this** for TTY detection; do not introduce a parallel `process.stdin.isTTY` check.
4. `zombiectl/src/lib/sse.js` — SSE reader to reuse for streaming activity responses inside the REPL loop.
5. `src/http/handlers/zombies/messages.zig` — the real steer endpoint: `POST /v1/workspaces/{ws}/zombies/{id}/messages`. Returns 202 with `event_id`. Live tail is a separate `GET .../activity` SSE stream.
6. `docs/architecture/data_flow.md` §"Steer flow end-to-end" — the two-call shape (XADD `zombie:{id}:events`, PUBLISH `zombie:{id}:activity`) the REPL iterates over.
7. `docs/v2/done/M63_004_P1_CLI_OBS_RESILIENCE.md` — patterns for SIGINT trapping + clean exit on CLI commands.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC (no dead code at write time), RULE UFS (consistent identifiers across CLI/Zig boundary).
- **`docs/BUN_RULES.md`** — TypeScript/Bun editing discipline for `zombiectl/`.

---

## Overview

**Goal (testable):** Running `zombiectl steer <zombie_id>` from an interactive terminal (`isTty(process.stdin)` true) opens a multi-turn prompt loop. Each line submitted produces a **two-call sequence**: `POST /v1/workspaces/{ws}/zombies/{id}/messages` returns 202 + `event_id`; `GET /v1/workspaces/{ws}/zombies/{id}/activity?since={event_id}` opens an SSE stream and drains chunks to stdout until a terminal event arrives (see `zombiectl/src/constants/event-status.js`). Prompt then re-displays. SIGINT closes the in-flight SSE connection and exits cleanly. Running the same command without a TTY (agent invocation, pipe, CI) behaves exactly as today — single-shot. The `--tty` flag forces REPL mode regardless of detected stdin.

**Problem:** Today, multi-turn steering from a terminal requires re-running `zombiectl steer <id> --message "..."` per turn. Agents (Claude Code, Codex, Amp, OpenCode) drive multi-turn naturally by re-invoking through their Bash tool — they don't need a REPL. A human at a terminal does. The asymmetry is unaddressed.

**Solution summary:** Add TTY detection to the existing `zombie steer` command. When `stdin.isTTY` is true (or `--tty` is set), enter a prompt loop: `> ` prompt → readline → POST steer → SSE drain to stdout → re-prompt. SIGINT (`Ctrl-C`) breaks the loop cleanly. Non-TTY behavior is unchanged. No server-side changes.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/zombie_steer.js` | EDIT | Replace the "interactive steer is not yet implemented" stub with the mode-dispatch table (see §1) and REPL loop. Single-shot path stays byte-identical. |
| `zombiectl/src/lib/sse.js` | EDIT | Default: no edit needed — reuse as-is. Edit only if the existing reader's lifecycle doesn't cleanly handle "drain to terminal event then return"; document the gap in PR Session Notes if an edit lands. |
| `zombiectl/src/lib/repl.js` (NEW) | CREATE | Small helper isolating the prompt-loop logic so it can be unit-tested without a real TTY. Imports `isTty` from `zombiectl/src/output/capability.js` (M64_001 infra) — no parallel TTY detection. |
| `zombiectl/tests/commands/zombie_steer_repl.test.ts` | CREATE | Tests below. |
| `docs/cli/reference/zombie.mdx` (docs repo) | EDIT | Document the new TTY behavior + `--tty` flag. |

---

## Sections (implementation slices)

### §1 — Mode dispatch (three modes + escape hatch)

`zombiectl steer <zombie_id> [<message>] [--tty]` resolves to one of four behaviors based on flags + stdin state. Single dispatch table at the top of the handler:

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Auto-REPL (human)** | `isTty(process.stdin)` true AND no positional `<message>` AND no `--tty` flag | Enter REPL loop (§2). |
| **Force-REPL** | `--tty` flag set (regardless of stdin state) | Enter REPL loop (§2). Fixture path: `echo "howdy" | zombiectl steer abc --tty` posts once, drains response, exits cleanly on stdin EOF. |
| **Agent / single-shot** | `isTty(process.stdin)` false AND no `--tty` flag | Today's behavior: positional `<message>` or piped stdin as one message, single two-call cycle, exit. Byte-identical to current single-shot. |
| **Explicit one-shot** | Positional `<message>` arg present (regardless of TTY state) | Single two-call cycle, then exit. Lets humans script non-interactively without negating the auto-REPL. |

**Implementation default:** reuse `isTty(process.stdin)` from `zombiectl/src/output/capability.js` (M64_001 infra). Do NOT introduce a parallel `process.stdin.isTTY === true` check — there is one source of truth for TTY detection across the CLI.

### §2 — REPL loop (two-call shape)

Each turn:

1. Print prompt (`> ` — match existing CLI prompt style).
2. Read one line from stdin. Empty line → skip (re-prompt without a POST).
3. `POST /v1/workspaces/{ws}/zombies/{id}/messages` with body `{ "message": "<line>" }`. Server returns **202 + `{ event_id }`**.
4. `GET /v1/workspaces/{ws}/zombies/{id}/activity?since={event_id}` opens an SSE stream (worker `PUBLISH`es to `zombie:{id}:activity` while reasoning — see `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel"). Stream chunks to stdout as they arrive.
5. Drain until a **terminal event** arrives (event types defined in `zombiectl/src/constants/event-status.js`). Close the SSE connection.
6. Re-prompt.
7. Loop until SIGINT or stdin EOF.

SIGINT handler must close any in-flight SSE connection before exiting, otherwise the server holds the stream open until idle timeout. Exit code 130 on SIGINT, 0 on stdin EOF.

### §3 — `--tty` force flag, EOF behavior

`zombiectl steer <zombie_id> --tty` enters REPL mode regardless of detected stdin state. On stdin EOF (piped input exhausted, or `Ctrl-D` at the prompt), REPL exits cleanly with code 0. Concrete fixture path: `echo "howdy" | zombiectl steer abc --tty` posts one message, drains the activity SSE to completion, exits 0.

### §4 — Non-TTY behavior unchanged

Mode 3 in the dispatch table. When `isTty(process.stdin)` is false AND `--tty` is not set, the command runs the existing single-shot logic (positional `<message>` or piped stdin as one message, single two-call cycle, exit). Agent invocation path is byte-identical to today's behavior — regression-tested in §5.

### §5 — Tests

Cover happy path (REPL responds + re-prompts), SIGINT clean exit, `--tty` flag forces REPL, non-TTY default falls through to single-shot. Use a mocked SSE source for determinism.

---

## Interfaces

**Command-line interface (locked):**

```
zombiectl steer <zombie_id>                       # Mode 1: TTY → REPL. Non-TTY → error.
zombiectl steer <zombie_id> "<message>"           # Mode 4: always single-shot (escape hatch).
zombiectl steer <zombie_id> --tty                 # Mode 2: force REPL regardless of TTY.
echo "msg" | zombiectl steer <zombie_id>          # Mode 3: piped stdin, single-shot.
echo "msg" | zombiectl steer <zombie_id> --tty    # Mode 2: force REPL, posts then exits on EOF.
```

**REPL session shape:**

```
$ zombiectl steer abc123
> howdy
[POST /v1/workspaces/{ws}/zombies/abc123/messages → 202 { event_id: evt_X }]
[GET .../activity?since=evt_X — SSE chunks stream here until terminal event]
> what's the slack channel id?
[POST → 202 { event_id: evt_Y }]
[GET .../activity?since=evt_Y — SSE chunks]
^C
$
```

**No server-side changes.** The endpoints `POST /v1/workspaces/{ws}/zombies/{id}/messages` (single ingress, returns 202 + event_id) and `GET /v1/workspaces/{ws}/zombies/{id}/activity?since={event_id}` (SSE drain) are unchanged.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| POST `.../messages` fails | Network blip, server 5xx | Print error to stderr, no `event_id` issued; return to prompt. User can retry. |
| Activity SSE disconnects mid-stream | Network blip during drain | Print "[stream disconnected]" to stderr; return to prompt. The prior `event_id` may still be processing server-side — operator can re-open via `zombiectl events --since {event_id}` if curious. |
| Auth token expired during long REPL session | `zombiectl auth login` expired | Print "auth expired — re-run `zombiectl auth login`," exit code 1. |
| User sends empty line | Pressed Enter at prompt | Skip — re-prompt without making a POST. |
| Zombie deleted during session | Concurrent action | Server returns 404; REPL prints "zombie no longer exists" and exits. |
| SIGINT during in-flight SSE | User Ctrl-C while response streams | Close SSE connection, print partial output already received, exit code 130. |
| Network error on initial connect | Server unreachable | Same handling as today's single-shot path. |

---

## Invariants

1. Non-TTY invocation without `--tty` behaves byte-identically to pre-M69_002 — enforced by regression test diffing pre/post output on the same fixture.
2. SIGINT during REPL always closes the SSE connection before process exit — enforced by a test that asserts no orphaned server-side stream remains.
3. No server-side endpoint changes — enforced by `git diff src/` returning empty for HTTP handlers.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_tty_detected_enters_repl` | With mocked `stdin.isTTY = true`, the command enters the prompt loop instead of single-shot. |
| `test_non_tty_falls_through` | With `stdin.isTTY = undefined`, the command runs single-shot exactly as today. Byte-equal output on fixture. |
| `test_repl_loops_until_sigint` | Mocked stdin emits 3 lines, then SIGINT. Asserts 3 message POSTs + 3 activity-SSE drains complete, process exits with 130. |
| `test_repl_exits_clean_on_stdin_eof` | Mocked stdin emits 1 line then closes (no SIGINT). Asserts 1 message POST + 1 SSE drain, process exits with 0. Covers `echo "x" | zombiectl steer abc --tty` fixture path. |
| `test_explicit_message_arg_single_shot` | `zombiectl steer abc "howdy"` with `isTty=true` → single two-call cycle, exits 0. Validates the escape-hatch mode (positional message overrides auto-REPL). |
| `test_force_tty_flag` | `--tty` with `stdin.isTTY = undefined` enters REPL. |
| `test_sigint_closes_sse_in_flight` | SIGINT while SSE response is mid-stream → SSE connection's `close()` is called before process exit. |
| `test_empty_line_skipped` | Empty stdin line at prompt does NOT trigger a POST; re-prompts immediately. |
| `test_server_404_during_repl` | Zombie deleted mid-session → server returns 404 → REPL prints message + exits 1. |
| `test_auth_expired_during_repl` | Server returns 401 → REPL prints "re-run auth login" + exits 1. |

---

## Acceptance Criteria

- [ ] `bun test` in `zombiectl/` passes — verify: `cd zombiectl && bun test`.
- [ ] TTY detection verified manually with a real terminal — paste session transcript in PR Session Notes.
- [ ] Non-TTY behavior unchanged — verify: regression test green.
- [ ] `make lint` clean.
- [ ] `gitleaks detect` clean.
- [ ] No file over 350 lines added.

---

## Eval Commands

```bash
# E1: zombiectl tests
cd zombiectl && bun test 2>&1 | tail -5

# E2: lint
make lint 2>&1 | tail -3

# E3: TypeCheck
cd zombiectl && bun run typecheck 2>&1 | tail -3

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3

# E5: 350L gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 }'
```

---

## Dead Code Sweep

N/A — no files deleted, only added/edited.

---

## Skill-Driven Review Chain

| When | Skill | Required output |
|------|-------|-----------------|
| Pre-CHORE(close) | `/write-unit-test` | Coverage clean against the Test Specification above. |
| Pre-CHORE(close) | `/review` | Adversarial review against `docs/BUN_RULES.md` + REPL state-machine edge cases. |
| Post `gh pr create` | `/review-pr` | Greptile pass. |

---

## Verification Evidence

(Filled during VERIFY.)

---

## Out of Scope

- `--ndjson` machine-readable streaming on the single-shot path (separate spec if Captain wants agent-friendly structured output). M69_002 leaves the single-shot path byte-identical.
- Dashboard chat REPL (lives entirely in M68 §5 — different surface, different audience).
- Per-zombie history / scrollback inside the REPL (premature).
- Tab-completion of commands inside the REPL (premature).
