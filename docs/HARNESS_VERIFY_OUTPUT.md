# HARNESS VERIFY — Required output block

> Parent: [`../AGENTS.md`](../AGENTS.md) §HARNESS VERIFY.

Runs after EXECUTE, before VERIFY. Aggregates every gate verdict; lifecycle cannot advance without enumerating the audit. Any "fail" / remaining violations → return to EXECUTE.

## Combined end-of-turn audit

Single awk over `git diff -U0 HEAD`, replaces 4 separate self-audits. For every `+` line:

- Emit `MS-ID:` hits when file is source/config and matches `M[0-9]+_[0-9]+|§[0-9]+(\.[0-9]+)+|\bT[0-9]+\b|\bdim [0-9]+\.[0-9]+\b`.
- Emit `PUB:` hits when `*.zig` and line matches `^\+(pub | *pub fn | *[A-Z][a-zA-Z]+,$)`.
- Emit `UI:` hits when under `ui/packages/app/**.{tsx,jsx}` and line contains `<(section|button|input|dialog|article|nav|header|form)\b`.

Non-empty = address before HARNESS VERIFY passes.

## Required output

Verdict cells use ✅ pass · ⚪ n/a · 🔴 fail · 🟡 violations addressed.

```
🚧 HARNESS VERIFY: <branch>
| Gate                 | Verdict                       |
| FILE SHAPE           | ✅ pass | ⚪ n/a               |
| PUB GATE             | ✅ pass | ⚪ n/a               |
| LENGTH GATE          | ✅ pass | 🟡 files at cap: …    |
| MILESTONE-ID GATE    | ✅ pass — combined audit, 0 hits |
| ZIG GATE             | ✅ pass | ⚪ n/a               |
| UI GATE              | ✅ pass | ⚪ n/a               |
| DESIGN TOKEN GATE    | ✅ pass | 🟡 N arbitraries addressed | 🔴 N unresolved |
| UFS GATE             | ✅ pass | 🟡 N violations addressed | 🔴 N unresolved |
| SCHEMA GUARD         | ✅ pass | ⚪ n/a               |
| GREPTILE GATE        | ✅ pass | 🟡 N violations addressed |
| Architecture consult | ✅ doc updated same commit | ⚪ n/a |
| Coverage             | ✅ backend N% ≥ min · UI N% ≥ min | ⚪ n/a |
| /write-unit-test     | ✅ clean | 🟡 N tests added   |
```

Any 🔴 in the table → return to EXECUTE; the lifecycle does NOT advance.
