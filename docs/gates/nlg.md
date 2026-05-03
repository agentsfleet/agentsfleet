# 🚧 RULE NLG — No legacy framing pre-v2.0.0

**Family:** Legacy-control. Sibling rules: **RULE NDC**, **RULE NLR**, **Legacy-Design Consult Guard**. **Source:** `docs/greptile-learnings/RULES.md` RULE NLG.

**Triggers:** introducing any new `legacy_*` name, `V2`-twin type, `if (legacy_caller)` branch, backward-compat shim, "rejecting legacy X" prose, or violation tracking-list while `cat VERSION` < `2.0.0`.

**Override:** `RULE NLG: SKIPPED per user override (reason: ...)` immediately preceding the edit. **User-invokable only.** Requires a concrete external consumer that can't migrate same-commit (vanishingly rare pre-v2.0.0).

## What this gate enforces

While `cat VERSION` < `2.0.0`, the project has no external consumers and no published API. Do not introduce *new* legacy concepts in any form:

- No `legacy_*` error variant names.
- No `if (legacy_caller)` branches.
- No `V2`-suffixed twin types.
- No backward-compat shims.
- No "rejecting legacy X" prose in specs/docs/commit messages.

Edit interfaces in place; update every caller in the same commit. Name errors by *what is wrong*, not *when it was wrong* (e.g. `runtime_keys_outside_block`, not `legacy_top_level_runtime`).

## Why

Pre-alpha duplicates rot faster than documentation. Every `legacy_*` name introduces a phantom interface nobody owns; every future spec then has to reason about it. Schema Table Removal Guard already encodes this for SQL — RULE NLG generalises it to every interface (RPC, route, struct, error name, config key, spec prose).

## Tracking-list ban

Any constant, doc structure, or carve-out list whose purpose is to catalog "violations to be cleaned up later" is itself an NLG violation. Banned name patterns: `LEGACY_`, `PENDING_`, `_VIOLATIONS`, `_CARVE_OUTS` (when meant for deferred cleanup), `TO_FIX_`, `DEFERRED_`, or any equivalent.

Either fix every entry in the same diff that introduces or touches the list, or delete the list and let the next touch fix the underlying violations. The tracking list **legitimises deferral**; the rule exists to prevent that.

**Vendor-immortal carve-outs** — paths or names dictated by external commitments that genuinely cannot be renamed (e.g. OAuth callback URLs that Slack/GitHub register with us) — are a separate class. Name those explicitly with `VENDOR_` or `EXTERNAL_` prefix so the distinction from "deferred cleanup" is mechanical, and add a comment line stating the external commitment that pins the name.

## Required output (when violation found)

```
NLG: <file>:<line> | new legacy framing: <description> | action: rename | suggested name: <what-is-wrong-form>
```

## Full text

`docs/greptile-learnings/RULES.md` RULE NLG.
