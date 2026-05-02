# 🚧 Legacy-Design Consult Guard

**Family:** Legacy-control. Sibling rules: **RULE NDC**, **RULE NLR**, **RULE NLG**. **Source:** `AGENTS.md` (project-side guard, not a greptile-derived rule).

**Triggers** — STOP and consult before any of these:

- Patching legacy to fit new architecture ("compensating code").
- Keeping legacy for "backward compat" pre-alpha when there are no external consumers.
- Defensive `orelse` / fail-open whose only reason is legacy nullability.
- Authoring tests that exercise the legacy path.
- Choosing patch-vs-remove silently — never your call.

**Override:** none — user decides A/B/C in the consult block.

## Definition — "legacy design"

Any code path, env-var, table, route, or API that the surrounding milestone work is deprecating, that predates the current architectural direction, or that exists solely as a smoke-test / bootstrap / pre-migration shim. Signals:

- Comments like `// legacy`, `// pre-M*`, `// bootstrap`, `// TODO remove`, `// temporary`.
- Runtime warn logs that announce themselves (`legacy path`, `deprecated`, `*_bootstrap_*`).
- Env vars / principals / roles / schema cols whose only live consumer is a fallback branch or pre-signup/dev-only path.

## Required output

```
LEGACY CONSULT: <desc> | found:<file:line> | (A) remove [blast:<files>] / (B) patch [risk] / (C) keep [why] | rec:<A|B|C> because <reason> | WAITING.
```

Block on the user's reply. If the user previously approved one *class* of legacy decisions this session, note that and proceed — but every *new* class of finding still triggers a consult.

## Escape hatch

Legacy findings unambiguously in-scope of the active spec's Dead Code Sweep or Out-of-Scope list skip the consult and follow the spec.

## Discovery capture

Every triggered consult is logged in the active spec's **Discovery** section, or filed as a new pending spec in `docs/v{N}/pending/` if pushed to follow-up.

## Family

NLR is the cleanup-on-touch arm; NLG bans new legacy framing pre-v2.0.0; this guard covers the harder judgment calls ("should this whole subsystem exist") that need the user's input.
