# 🚧 RULE NLR — No legacy retained (touch-it-fix-it)

**Family:** Legacy-control. Sibling rules: **RULE NDC** (no dead code at write time), **RULE NLG** (no new legacy framing pre-v2.0.0), **Legacy-Design Consult Guard** (judgment calls on whole subsystems). **Source:** `docs/greptile-learnings/RULES.md` RULE NLR.

**Triggers:** any Edit/Write to a file containing pre-existing legacy framing or dead code.

**Override:** `RULE NLR: SKIPPED per user override (reason: ...)` immediately preceding the edit. **User-invokable only.** Concrete external-impact constraint OR explicit NLR DECISION resolution required. Generic "scope creep"/"too big" not valid.

## What this gate enforces

Any edit to a file with pre-existing legacy framing or dead code MUST remove it in the same diff. Patterns covered:

- `?*T = null` fields whose only caller always sets a non-null value (dead defense for a phantom caller).
- `legacy_*` symbol names, `V2`-suffixed twin types, `if (legacy_caller)` branches.
- `// legacy` / `// pre-M*` / `// bootstrap` comments.
- Runtime warns saying `legacy path` / `deprecated` / `*_bootstrap_*`.
- `pub` fns/fields with no in-tree consumer (verify with `grep -rn`).
- `defer if (x) ... else null` patterns that compensate for an `?T` that should have been `T`.
- Unused parameters, captures, unreachable branches.

The carve-out "pre-existing violations are not the agent's responsibility" does **not** apply when the agent is already touching the file. You see it, you own it.

## Pre-edit check

1. Before the first `Edit`/`Write` to a file, scan for the patterns above in the *whole file*, not just the lines you're touching.
2. List violations in the gate output before the edit.
3. Remove them in the same diff. Update every caller in the same commit.
4. If cleanup is judged infeasible (large net-line delta, cross-package cascade, meaningfully different design path emerges), print the **NLR DECISION** block below and **wait**. The agent has no autonomous escape; the user is the only authority.

## Decision block

```
NLR DECISION: <file>
  Cleanup-in-place cost: +<N> net lines, <M> files touched.
  Alternative approach: <one-line description> (avoids the dirty file).
  If alternative chosen, legacy that survives:
    - <symbol/pattern>: <file:line>
    - ...
  WAITING for user: clean / alternative.
```

## Required output (when violations found before edit)

```
NLR: <file> | violations: <list with file:line each> | action: clean-in-diff
```

## Anti-evasion clause

Three failure modes the agent MUST NOT use to skip cleanup:

1. **Route-around design.** Picking an architecture that avoids the dirty file specifically to dodge cleanup. If NLR avoidance was a motivation, surface it.
2. **Silent rejection.** Quietly choosing not to touch a file because cleanup looked expensive, without disclosing the decision.
3. **Shim-and-skip.** Introducing a wrapper/adapter to sidestep a dirty interface — the new code becomes its own legacy debt; the original rots in place.

If any of these patterns is in play, surface it via the NLR DECISION block. The user retains all discretion.

## Family

- **RULE NDC** — prevention at write time (don't author dead code).
- **RULE NLG** — ban new legacy framing pre-v2.0.0.
- **Legacy-Design Consult Guard** — harder judgment calls ("should this whole subsystem exist") that need user input.
- **RULE NLR** — mechanical cleanup on touch.
