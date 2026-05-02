# 🚧 Invariance Suite Gate (meta-gate)

**Family:** Meta-gate — protects the gate ruleset itself. Sibling: none (this gate has no peer; it watches every other gate's source-of-truth files).
**Source:** `AGENTS.md` (project-side guard). Questionnaire body: `AGENTS_INVARIANCE.md` at repo root.

**Triggers:** any Edit/Write in this session to **any one of**:

- `AGENTS.md`
- `AGENTS_INVARIANCE.md`
- any file under `docs/gates/`
- `scripts/audit-agents-md.sh` or `scripts/fixtures/*.diff`

The trigger is **agent-self-edit**: the moment the agent *itself* modifies one of these files in the current session, this gate fires. (User-only edits do not fire it from the LLM's perspective — but the pre-push hook still gates them.)

**Override:** none from the agent side. The user MAY bypass at push time with `SKIP_INVARIANCE_PUSH=1 git push ...`, documented with a reason in the most recent commit message. The agent MUST NOT self-bypass.

## Required action (in-session, BEFORE declaring complete)

The agent runs these steps in order. Each step's output appears in the user-facing message.

1. **Script layer** — run `bash scripts/audit-agents-md.sh`. If exit ≠ 0, STOP, fix the FAIL lines, retry. Do NOT proceed to step 2 with a failing script.

2. **Questionnaire layer** — read `AGENTS_INVARIANCE.md` and answer every question against the *current* `AGENTS.md` (and gate bodies). Each answer is YES or NO with the justifying line(s).

3. **Tabulated report** — emit the Step-4 report from `AGENTS_INVARIANCE.md` (gates, rules, lifecycle, scenario verdicts, OVERALL).

4. **Sign-off** — only when **all questions answered YES** AND the report's OVERALL row is `PASS`. After the commit lands, write:

   ```bash
   printf '%s  %s  PASS\n' "$(git rev-parse --short HEAD)" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
     > .agents-invariance-signoff
   ```

   (The file is gitignored — it's a per-machine attestation, not tracked.)

5. **Tell the user** — surface that the sign-off was written and `git push` is now unblocked. If any question answered NO, surface the regression and STOP — the user decides whether to fix or override.

## Sequencing relative to commit & push

The sign-off must be tied to the **post-commit HEAD SHA**, so the order is:

1. Edit ruleset files.
2. Step 1 (script) ✓.
3. Step 2 (questionnaire) ✓.
4. Step 3 (report) ✓.
5. `git commit` — pre-commit hook runs the script again (defence in depth).
6. Step 4 (sign-off) — written against the new HEAD.
7. `git push` — pre-push hook reads the sign-off; on PASS + matching SHA + < 24 h freshness, push proceeds.

## Why this gate exists

`AGENTS.md` is the agent's ruleset. Every other gate trusts that ruleset. If the ruleset drifts silently — a gate vanishes, a trigger silently drops a file class, an override path widens — every downstream gate is now wrong. The script catches mechanical regressions; the questionnaire catches semantic regressions that need reading comprehension. Together they prove the ruleset still holds *before* the bad version reaches the remote.

## Failure modes the gate guards against

- A gate body is moved or renamed; the AGENTS.md pointer rots; agents silently skip the gate.
- An override path quietly widens (e.g. "user-only" → "auto-mode covers"); autonomy creep.
- The always-forbidden list shrinks (e.g. someone removes `gitleaks` enforcement).
- The skill-chain order shuffles (`/review-pr` runs before `/review`).
- The HARNESS VERIFY block stops listing a gate row.
- `AGENTS_INVARIANCE.md` itself loses scenarios.

## Required output (in-session)

```
🚧 INVARIANCE SUITE GATE: <branch> @ <head-sha>
  Step 1 (script):        ✅ ALL CHECKS PASSED
  Step 2 (questionnaire): ✅ <N>/<N> YES
  Step 3 (report):        emitted
  Step 4 (sign-off):      ✅ written to .agents-invariance-signoff
```

Any 🔴 → STOP, surface, do NOT push.
