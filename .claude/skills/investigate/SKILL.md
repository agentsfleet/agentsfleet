---
name: investigate
version: 1.0.0
description: |
  Systematic debugging with root cause investigation. Four phases: investigate,
  analyze, hypothesize, implement. Iron Law: no fixes without root cause.
  Automatically scope-locks edits to the affected module via /freeze.
  Use when asked to "debug this", "fix this bug", "why is this broken",
  "investigate this error", or "root cause analysis".
  Proactively suggest when the user reports errors, unexpected behavior, or
  is troubleshooting why something stopped working.
allowed-tools:
  - Bash
  - Read
  - Write
  - Edit
  - Grep
  - Glob
  - AskUserQuestion
hooks:
  PreToolUse:
    - matcher: "Edit"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/../freeze/bin/check-freeze.sh"
          statusMessage: "Checking debug scope boundary..."
    - matcher: "Write"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/../freeze/bin/check-freeze.sh"
          statusMessage: "Checking debug scope boundary..."
---

# /investigate — Systematic Debugging

## Iron Law

**NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST.**

Fixing symptoms creates whack-a-mole debugging. Find the root cause, then fix it.

---

## Phase 1: Root Cause Investigation

1. **Collect symptoms** — read error messages, stack traces, reproduction steps. If insufficient context, ask ONE question at a time.
2. **Read the code** — trace the path from symptom back to potential causes. Use Grep to find all references.
3. **Check recent changes:**
   ```bash
   git log --oneline -20 -- <affected-files>
   ```
   Was this working before? A regression means the root cause is in the diff.
4. **Reproduce** — can you trigger the bug deterministically? If not, gather more evidence.

Output: **"Root cause hypothesis: ..."** — a specific, testable claim.

---

## Scope Lock

After forming a hypothesis, lock edits to the affected module to prevent scope creep.

Identify the narrowest directory containing the affected files. Write it:

```bash
STATE_DIR="${CLAUDE_PLUGIN_DATA:-$HOME/.gstack}"
mkdir -p "$STATE_DIR"
echo "<detected-directory>/" > "$STATE_DIR/freeze-dir.txt"
echo "Debug scope locked to: <detected-directory>/"
```

Tell the user: "Edits restricted to `<dir>/` for this debug session. Run `/unfreeze` to remove."

**Instruction enforcement:** Before editing any file outside this directory, refuse and explain. Do not rely solely on the hook.

---

## Phase 2: Pattern Analysis

Check if the bug matches a known pattern:

| Pattern | Signature | Where to look |
|---------|-----------|---------------|
| Race condition | Intermittent, timing-dependent | Concurrent access to shared state |
| Nil/null propagation | panic, null deref | Missing guards on optional values |
| State corruption | Inconsistent data, partial updates | Transactions, callbacks |
| Integration failure | Timeout, unexpected response | External API calls, service boundaries |
| Configuration drift | Works locally, fails in staging | Env vars, feature flags, DB state |

Also check:
- `TODOS.md` for related known issues
- `git log` for prior fixes in the same area — **recurring bugs in the same files are an architectural smell**

---

## Phase 3: Hypothesis Testing

Before writing ANY fix, verify the hypothesis.

1. Add a temporary log or assertion at the suspected root cause. Run the reproduction.
2. Does the evidence match? If not — return to Phase 1.
3. **3-strike rule:** If 3 hypotheses fail, STOP. Ask the user:
   ```
   3 hypotheses tested, none match. Options:
   A) Continue — new hypothesis: [describe]
   B) Escalate — needs someone who knows the system deeply
   C) Instrument and wait — add logging, catch it next time
   ```

**Red flags — slow down if you see these:**
- Proposing a fix before tracing the data flow (you're guessing)
- Each fix reveals a new problem elsewhere (wrong layer, not wrong code)
- "Quick fix for now" — there is no "for now"

---

## Phase 4: Implementation

1. Fix the root cause, not the symptom. Smallest change that eliminates the actual problem.
2. Minimal diff — fewest files, fewest lines. Resist refactoring adjacent code.
3. Write a regression test that **fails without the fix** and **passes with it**.
4. Run the full test suite. No regressions allowed.
5. If the fix touches >5 files — ask the user:
   ```
   This fix touches N files — large blast radius for a bug fix.
   A) Proceed — root cause genuinely spans these files
   B) Split — fix critical path now, defer the rest
   C) Rethink — is there a more targeted approach?
   ```

---

## Phase 5: Verification & Report

Reproduce the original bug scenario and confirm it's fixed. Run the test suite.

```
DEBUG REPORT
════════════════════════════════════════
Symptom:         [what the user observed]
Root cause:      [what was actually wrong]
Fix:             [what was changed, with file:line]
Evidence:        [test output confirming fix]
Regression test: [file:line of the new test]
Related:         [TODOS.md items, prior bugs in same area]
Status:          DONE | DONE_WITH_CONCERNS | BLOCKED
════════════════════════════════════════
```

---

## Rules

- Never apply a fix you cannot verify. If you can't reproduce and confirm, don't ship it.
- Never say "this should fix it." Prove it. Run the tests.
- 3+ failed fix attempts → STOP and question the architecture.
- Completion status: **DONE** (verified) / **DONE_WITH_CONCERNS** (can't fully verify) / **BLOCKED** (escalate)
