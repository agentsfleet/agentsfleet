# write_spec.md — spec-authoring dispatch (LATENT façade)

This is the prose the AGENT reads **before writing or editing a spec** under
`docs/v*/{pending,active,done}/`. It pairs with the deterministic half
`audits/spec-template.sh`, which runs in `make lint`. (This is the former Spec Template gate absorbed into
the dispatch model — `docs/TEMPLATE.md` remains the canonical section source.)

**Signal legend:**

- 🟢 / 🔴 — `audits/spec-template.sh` mechanically passes/fails the prohibited-pattern and
  required-section checks.
- 🔵 DECIDE — whether a required section is *meaningfully filled* (not just
  present) is the agent's judgment at author time.

## Trigger — every Edit/Write to

- Any spec under `docs/v*/{pending,active,done}/**/*.md`.
- `docs/TEMPLATE.md` itself.
- Any `*.md` whose body carries the spec frontmatter pattern (`^**Milestone:** M\d+`).

**Override:** `SPEC TEMPLATE GATE: SKIPPED per user override (reason: ...)`.
**User-invokable only.** Auto-mode does NOT cover it; reasons must cite a concrete
external constraint (vendor doc requirement, regulatory format), never internal
preference.

## What this covers

**Family 1 — prohibited patterns (negative space).** `docs/TEMPLATE.md`
"Prohibited" forbids: time/effort estimates, effort columns, complexity ratings,
percentage-complete fields, assigned owners, implementation dates. Specs created
from the template don't carry the Prohibited section forward — only the body
below the divider gets copied — so without enforcement these drift back in.

**Family 2 — required-present + no-placeholder (positive space).** The
agent-facing template mandates the determinism sections the executing agent reads
to emit invariant output: PR Intent & comprehension handshake, Applicable Rules,
Applicable Gates, Overview, Prior-Art / Reference Implementations, Files Changed,
Decomposition & alternatives, Sections, Metrics & Observability, Interfaces,
Failure Modes, Invariants, Test Specification, Acceptance Rubric (legacy heading `Acceptance Criteria` accepted), Product Clarity (authoring record), Discovery. A spec missing one — or
leaving template residue (`path/to/file.ext`, `test_<short_name>`,
`{one-line reason}`, any surviving `tpl:` guidance comment — the template's fill
grammar deletes them all at authoring) — forces the agent to guess intent.

**Scope (no legacy carve-out).** Family 2 fires in `--staged` only — the spec
being authored now (the agent's own output): BLOCK. Bulk scans (`--all` /
`--include-done`, which `make harness-verify` runs) execute Family 1 only, so they
behave identically over the corpus and never break an existing spec.

## Pre-edit check

| Pattern | Rule |
|---|---|
| `## Estimated effort` (any heading variant) | Forbidden. Delete the section. |
| `## Effort` / `## Complexity` / `## Sizing` | Forbidden. Use Priority. |
| `\b\d+\s*[-–]\s*\d+\s*h\b` (hour ranges like "10-14h") | Forbidden. |
| `\b\d+\s*hours?\b` / `\b\d+\s*days?\b` / `\b\d+\s*min(utes?)?\b` | Forbidden. |
| `\b(low|medium|high|small|large)\s*[:|-]\s*(effort|complexity)\b` | Forbidden. |
| `\d+%\s*complete` / `\b\d+/\d+\b\s*tasks` | Forbidden — binary PENDING/IN_PROGRESS/DONE. |
| `**Owner:**` / `**Assigned to:**` | Forbidden — use git history. |
| `**Due:**` / `**Deadline:**` (with date) | Forbidden — use Priority. |
| Missing a required determinism section in a **staged** spec | BLOCK — add it. |
| Unfilled template residue in a **staged** spec | BLOCK — fill the section. |
| Surviving `tpl:` guidance comment in a **staged** spec | BLOCK — delete it; the executor reads instance content only. |
| Spec without the SPEC AUTHORING RULES banner | Informational — reminds future edits. |

## Required output (default — one line)

```
SPEC TEMPLATE GATE: <file> | prohibited:<0|list> required-sections:<all-present|missing-list — staged only> placeholders:<0|list> banner:<yes|no>
```

Full multi-line block fires on violation (prohibited sections / time estimates /
effort fields / %-complete / owner-date fields, each with line numbers, plus the
`audits/spec-template.sh` staged-diff finding count).

## Scope (M70)

`audits/spec-template.sh` walks the **full pending+active spec set** via `git ls-files`;
the index includes staged-but-uncommitted content, so a fix staged in pre-commit
satisfies the check on the same hook run. `--staged` is the opt-in narrowing mode.

## Authoring discipline (judgment layer)

Incident-derived rules the deterministic half can't check — apply while writing the spec, not after:

- **Validate intent against the repo, not the words.** Grep the codebase for the existing meaning of the spec's key nouns ("e2e", "acceptance", "live") before encoding them. The **duplicate-target smell**: a new target/recipe that comes out byte-identical to an existing one means the abstraction is wrong — STOP and surface (M74_004 made `live-e2e-all` identical to `test-integration`). And verify lanes in the real CI environment — a green local `docker compose` run proves nothing about a compose-less CI container.
- **Teardown / rename / flip specs open with a blast-radius grep.** `git grep -rn -w '<token>'` from repo root, no path or file-type filter; every surviving hit lands in Files Changed with production and test files separated. Word-boundary, never quoted-literal (YAML / multiline-string refs carry no quotes); grep filenames as tokens, never path-anchored (same-directory `@import("foo.zig")` has no path prefix); separate true targets from same-spelling-different-meaning hits (skill ref vs repo slug). The spec's invariant/rubric greps must use the same pattern as the discovery grep. (Incidents: M34_001, M62_001, M75_001.)
- **Grep-gate carve-outs for English.** A `\b(word)\b` zero-match gate overreaches when the word has a common English meaning ("run" is the usual offender). Encode the intent as enumerated product phrases (`run interrupt`, `spec init`, `gate loop`); when a literal gate fires on legitimate English, amend the spec — don't contort the prose.
- **No pre-/post-milestone or "production today" framing while pre-launch** — there is no production baseline to anchor a current-vs-future split against. Describe the target design as *the* design; mark a superseded model neutrally ("this file describes the single-process model; the M80 split supersedes it"), never as "pre-M80 / operational truth".
- **Security-boundary or backend-heavy follow-ups get their own spec + PR.** RBAC, secret reveal, account/tenant deletion, auth webhooks, new endpoints/scheduled jobs/billing policy — they carry a different review profile (AUTH chain, focused diff, own greptile pass) and never fold into a UI-polish PR, even after a "just fold it in". Small pure-UI items bundle fine; restoring a folded spec to `pending/` is cheap.
- **Rubric rows are outcomes, not Dimensions.** The Acceptance Rubric is the spec's single scoring surface: 5–12 rows (Section outcomes, failure classes, hygiene gates); every Expected mechanically checkable (exit code / literal substring / match count); Graded = ✅/❌ + one decisive output line. Per-Dimension proof lives in the Test Specification; evidence walls and per-Dimension rubric rows are violations.

## Family

- `docs/TEMPLATE.md` — canonical Prohibited section + required sections.
- `kishore-spec-new` skill — creates specs from the template; inserts the banner.
- `audits/spec-template.sh` — mechanical regex enforcement, runs in `make lint`.
