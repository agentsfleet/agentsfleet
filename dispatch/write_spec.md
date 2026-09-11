# Spec authoring

Read this before editing a spec under `docs/v*/{pending,active,done}/`, or
`docs/TEMPLATE.md`. The template defines the document shape. The authoring skill
fills it; the audit checks structural consistency. None proves design quality.

## What must be ready, and when

| Stage | Required content |
|---|---|
| Authoring | Goal, scope, verified source references, enforcement mechanisms and failure assumptions, decisions and acyclic prerequisites, Dimensions with test mappings, failures, interfaces, and acceptance checks |
| CHORE(open) | Active status, branch, full comparison revision, baseline measurement pending |
| PLAN | Implementer's comprehension handshake; resolved scope and decisions needed by the next Section |
| Before Pull Request | Declared unit/integration baseline counts, comparison revision, and evidence reference |
| Completion | Passing behavior proofs, final repository checks, and any required human evidence |

The sequence and hydration timing live in `dispatch/lifecycle.md`. A preparation
Section may establish inputs for later Sections when the spec states its output
and who may decide it. A required human sign-off remains a human dependency.

## Mechanical checks

`audits/spec-template.sh` is the entry point; its Bun helper
`audits/spec-template.ts` reads headings, lists, and tables from the runtime's
parsed Markdown tree. Code examples and quoted headings cannot supply sections.

- Prohibited authoring estimates, ownership labels, and completion percentages.
- Required sections with content, known placeholder residue, and the spec length cap.
- Existing repository files or URLs in the read-first list. Planned CREATE files
  belong in Files Changed, not in the existing-reference list.
- Each Dimension names a proof and has a matching tiered Test Specification row
  with an assertion. Duplicate identifiers and orphan test rows fail.
- The rubric's Verify cells contain the exact declared `conform` and `verify.*`
  commands, with Expected values. Include conditional suites; the final gate
  decides their applicability from the actual branch diff. Additional
  spec-specific commands and explicitly named manual checks remain valid.

`--staged` reads spec contents, configuration, and local references from the Git
index. `--file <path>` checks the working copy. Missing runtime or configuration
fails these readiness checks. Bulk `--all` and `--include-done` scans retain
prohibited-pattern checks only; they do not retrofit historical documents.

## Author review

Check that required content is meaningful, prerequisites are attainable, failures
are tested, and the acceptance checks prove the goal. A structural pass does not
mean the feature is implemented. Use `manual` for a human proof; record its
procedure, required person, and durable evidence without inventing sign-off.

Apply docs/TEMPLATE.md's Implementation readiness review to the relevant surfaces.
Pin versions, constraints, privileges and ordering when changing them could
invalidate a proof; omit routine coding recipes. Verify actual client/server
behavior and distinguish source inspection from runtime evidence.
For state replacement, name the authority, commit/release boundary, retry rules,
expiry/deletion behavior and fresh/populated upgrade path. Read the real migrator
and deployment release command before calling an image rollback a safe abort.
Budget the complete request path against an identified comparable reference.
Record direct user decisions separately from reviewer suggestions; a platform
risk needs an explicit disposition, not inferred acceptance. Attack the amended
spec for counterexamples and stale requirements before reporting readiness.

Use `{{fill:description}}` for authoring slots. Delete them and all `tpl:`
guidance before staging. Runtime parameters such as `/items/{id}` are allowed
when the spec defines them. Lifecycle-owned values use explicit pending text,
not unfilled authoring slots. Existing specs follow current rules; examples do
not override them.

**Override:** `SPEC TEMPLATE GATE: SKIPPED per user override (reason: ...)`.
User-invokable only; an external constraint must explain the exception.

## Required output

Report structural findings with file paths. Report an implementation verdict
only after the spec's behavior and repository checks have run.

## Authoring discipline (judgment layer)

Incident-derived rules the deterministic half can't check — apply while writing the spec, not after:

- **Validate intent against the repo, not the words.** Grep the codebase for the existing meaning of the spec's key nouns ("e2e", "acceptance", "live") before encoding them. The **duplicate-target smell**: a new target/recipe that comes out byte-identical to an existing one means the abstraction is wrong — STOP and surface (one incident's new e2e umbrella came out byte-identical to the existing integration target). And verify lanes in the real CI environment — a green local `docker compose` run proves nothing about a compose-less CI container.
- **Teardown / rename / flip specs open with a blast-radius grep.** `git grep -rn -w '<token>'` from repo root, no path or file-type filter; every surviving hit lands in Files Changed with production and test files separated. Word-boundary, never quoted-literal (YAML / multiline-string refs carry no quotes); grep filenames as tokens, never path-anchored (same-directory `@import("foo.zig")` has no path prefix); separate true targets from same-spelling-different-meaning hits (skill ref vs repo slug). The spec's invariant/rubric greps must use the same pattern as the discovery grep.
- **Grep-gate carve-outs for English.** A `\b(word)\b` zero-match gate overreaches when the word has a common English meaning ("run" is the usual offender). Encode the intent as enumerated product phrases (`run interrupt`, `spec init`, `gate loop`); when a literal gate fires on legitimate English, amend the spec — don't contort the prose.
- **No pre-/post-milestone or "production today" framing while pre-launch** — there is no production baseline to anchor a current-vs-future split against. Describe the target design as *the* design; mark a superseded model neutrally ("this file describes the single-process model; the M80 split supersedes it"), never as "pre-M80 / operational truth".
- **Security-boundary or backend-heavy follow-ups get their own spec + PR.** RBAC, secret reveal, account/tenant deletion, auth webhooks, new endpoints/scheduled jobs/billing policy — they carry a different review profile (AUTH chain, focused diff, own greptile pass) and never fold into a UI-polish PR, even after a "just fold it in". Small pure-UI items bundle fine; restoring a folded spec to `pending/` is cheap.
- **Rubric rows are outcomes, not Dimensions.** The Acceptance Rubric is the spec's single scoring surface: 5–12 rows (Section outcomes, failure classes, hygiene gates); every Expected mechanically checkable (exit code / literal substring / match count); Graded = ✅/❌ + one decisive output line. Per-Dimension proof lives in the Test Specification; evidence walls and per-Dimension rubric rows are violations.

## Family

- `docs/TEMPLATE.md` — canonical Prohibited section + required sections.
- `orly-spec-new` skill — creates specs from the template; inserts the banner.
- `audits/spec-template.sh` — mechanical regex enforcement, runs in `make lint`.
