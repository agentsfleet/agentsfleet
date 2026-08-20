<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M171_001: Every command-line interface (CLI) command rejects a bad invocation through one error shape and one exit code

**Prototype:** v2.0.0
**Milestone:** M171
**Workstream:** 001
**Date:** Aug 19, 2026
**Status:** DONE
**Priority:** P1 — the rejection surface is the first thing a new operator meets, and it currently answers in two dialects
**Categories:** CLI
**Batch:** B1 — standalone; no other workstream touches `cli/src/cli.ts`
**Branch:** feat/m171-cli-arg-rejection
**Test Baseline:** unit=4157 integration=709
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5, Aug 19, 2026) from a hand-rolled probe session by Indy
**Canonical architecture:** `docs/architecture/cli.md` — the CLI surface and its output discipline

---

## Overview

**Goal (testable):** every `agentsfleet` invocation rejected for argument shape — missing positional, missing required option, missing option value, malformed identifier, unknown command — prints `✕ error: <what>` followed by `  Suggestion: usage: agentsfleet <the fix>` and exits 4, and every group node invoked bare prints its help on stdout at exit 0.

**Problem:** the same operator mistake answers in two dialects. `agentsfleet logs` says `✕ error: logs requires --fleet <id>` and exits 4; `agentsfleet events` says `error: missing required argument 'fleet_id'` and exits 2. Roughly twenty commands speak commander's raw dialect with no suggestion line at all, so the operator learns what is missing but never how to obtain it. Exit 2 is worse than inconsistent: `EXIT_CODE` already assigns 2 to `NetworkError`, so a script cannot distinguish "you typed it wrong" from "the daemon is unreachable". Bare group nodes compound it by writing help to stderr while exiting 0, so `agentsfleet workspace | less` shows an empty page.

**Solution summary:** one central re-render, no per-handler rewrite. `applyOutputToTree` already walks every subcommand to install `exitOverride` and `configureOutput`; it gains an `outputError` hook that reformats commander's own rejection text into the house `✕ error:` / `Suggestion:` shape, and `exitFromCommanderError` maps usage codes to the validation exit code instead of 2. The `<required>` declarations stay exactly as they are, so `--help` and usage text keep working. Three handler-side messages that already speak the house shape are repaired where their suggestion echoes the detail. A table-driven acceptance spec in the deterministic lane then proves the shape for every command, so the negative paths stop depending on a live API target.

## PR Intent & comprehension handshake

- **PR title (eventual):** unify CLI argument rejection on one error shape and exit code
- **Intent (one sentence):** an operator who mistypes any `agentsfleet` command gets the same readable answer telling them what is missing and how to supply it, and a script can tell a bad invocation apart from a dead network.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/src/cli.ts` — `applyOutputToTree` (the tree walk that already owns every subcommand's output) and `exitFromCommanderError` + `COMMANDER_USAGE_CODES` (the exit mapping). Both change; nothing else in the dispatch path does.
2. `cli/src/commands/fleet_install.ts` and `cli/src/commands/memory.ts` — the house rejection shape already done right (`✕ error: --library <id> is required` / `Suggestion: usage: agentsfleet install --library <id>`). Mirror this wording, do not invent a new one.
3. `cli/src/errors/index.ts` — the `CliError` taxonomy and the `EXIT_CODE` map that makes validation 4 and network 2.
4. `cli/test/acceptance/fixtures/command-matrix.ts` — the single source of truth for command enumeration; the new sweep reads from it, and `acceptance-lanes.test.ts` will fail if the new spec file is not classified into a lane.
5. `cli/test/acceptance/help-and-errors.spec.ts` — the deterministic-lane pattern to copy: stubbed state dir plus an unroutable API base URL so a leaked fetch surfaces as a connection error instead of the expected stem.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/cli.ts` | EDIT | delegates the commander boundary to the new module and renders the captured rejection; drops below the length cap in the process |
| `cli/src/lib/commander-boundary.ts` | CREATE | the whole commander boundary: output wiring, rejection capture and render, exit mapping, bare-group resolution |
| `cli/src/constants/rejection.ts` | CREATE | rejection stems, usage prefix, and the stable `--json` codes, declared once (RULE UFS) |
| `cli/src/program/cli-tree.ts` | EDIT | group nodes route bare-invocation help to stdout instead of stderr |
| `cli/src/commands/fleet_logs.ts` | EDIT | the suggestion stops echoing the detail and names the fix |
| `cli/src/commands/grant.ts` | EDIT | suggestion adopts the `usage: agentsfleet …` form the other handlers use |
| `cli/src/errors/index.ts` | EDIT | imports the shared suggestion prefix instead of declaring its own copy (RULE UFS/NLR) |
| `cli/src/errors/auth.ts` | EDIT | same — the second of the two duplicate declarations |
| `cli/test/acceptance/argument-negatives.spec.ts` | CREATE | the table-driven negative sweep over every command |
| `cli/test/command-matrix-parity.unit.test.ts` | CREATE | walks the built tree and fails when the matrix falls behind it |
| `cli/test/acceptance/run-lane.ts` | EDIT | classify the new spec into the deterministic lane |
| `cli/test/acceptance/fixtures/command-matrix.ts` | EDIT | add `events`/`steer`, the group-node table, and the option-value-missing table |
| `cli/test/did-you-mean.integration.test.ts` | EDIT | unknown-command assertions move from exit 2 to the validation exit code |
| `cli/test/json-contract.test.ts` | EDIT | one usage-rejection exit assertion follows the same move |
| `cli/test/cli-funcfill.unit.test.ts` | EDIT | two runCli exit-mapping assertions follow the same move |
| `cli/test/pty.unit.test.ts` | EDIT | the spawned-process usage-exit constant follows the same move |
| `cli/test/fleet-logs-linecov.unit.test.ts` | EDIT | asserts the suggestion names the fix and differs from the detail |
| `cli/test/commander-boundary.unit.test.ts` | CREATE | in-process cover for the boundary; the acceptance sweep proves the same paths but runs in a subprocess the coverage floor cannot see |
| `docs/v2/active/M171_001_P1_CLI_ARGUMENT_REJECTION_UNIFORMITY.md` | EDIT | this spec — lifecycle status, Dimensions marked DONE, rubric graded |
| `cli/test/connector.integration.test.ts` | EDIT | fixture gains a configured-but-not-connected row so the list renderer's next-action text is asserted |
| `~/Projects/docs` (separate branch) | EDIT | the CLI reference page documents the exit-code table |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (the rejection stem, the `usage:` prefix, and every exit-code literal become named constants, declared once); **NDC** (no dead code: the old exit-2 branch is deleted, not left behind a flag); **NLR** (touch-it-fix-it — the `logs` echo is repaired in this diff, not deferred); **ORP** (orphan sweep once `COMMANDER_USAGE_CODES` changes role).
- **`~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE DECISION at PLAN for the new spec file; `const` and import discipline; Bun primitives for the subprocess runner (the existing `fixtures/cli.js` runner is reused, not re-rolled).
- **`~/Projects/dotfiles/dispatch/write_any.md`** — File & Function Length on `cli/src/cli.ts`, which is already the largest file in the dispatch path.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` in the diff | N/A |
| PUB / Struct-Shape | no — no new public module surface; the changes are internal to existing functions | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — `cli/src/cli.ts` grows and the new spec file is table-driven | the re-render lands as its own named function; if `cli.ts` approaches the cap the renderer moves to `cli/src/lib/commander-error-render.ts` alongside the existing `cli-error-render.ts` |
| UFS (repeated/semantic literals) | yes — stems, the `usage:` prefix, and exit codes repeat across source and spec | named constants in `cli/src/constants/`; the acceptance spec imports them rather than re-typing the strings |
| UI Substitution / DESIGN TOKEN | no — terminal output only | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no new `UZ-XXX-NNN` code; the rejection is client-side and carries no registry entry | N/A |

## Prior-Art / Reference Implementations

- **Reference:** `cli/src/commands/fleet_install.ts` and `cli/src/commands/memory.ts` — the house rejection shape, already shipping. This workstream propagates that wording rather than designing a new one.
- **Reference:** the "7 Pillars" of CLI developer experience — this diff aligns with *structured errors carrying a suggestion field* and *output as a service* (the renderer, not the handler, decides how a failure prints). Divergence: commander owns parse-time rejection, so the re-render happens at the output boundary instead of inside a handler; that keeps handler purity intact rather than breaking it.

## Sections (implementation slices)

### §1 — One rejection shape for every command

Commander's parse-time rejections are reformatted at the output boundary so they read like the handler-side ones. The `<required>` and `requiredOption` declarations are untouched, so help text, suggestion-after-error, and excess-argument detection all keep working. **Implementation default:** re-render inside `applyOutputToTree`'s `configureOutput`, because that walk already reaches every subcommand and is the one place that cannot be bypassed.

- **Dimension 1.1** [DONE] — a missing required positional prints `✕ error: …` and a `Suggestion:` line → Test `test_missing_positional_uses_house_shape`
- **Dimension 1.2** [DONE] — a missing required option and a missing option value take the same shape → Test `test_missing_option_uses_house_shape`
- **Dimension 1.3** [DONE] — an unknown command keeps its did-you-mean text inside the new shape → Test `test_unknown_command_keeps_suggestion`
- **Dimension 1.4** [DONE] — the suggestion always names the fix and never repeats the detail verbatim → Test `test_suggestion_never_echoes_detail`
- **Dimension 1.5** [DONE] — in `--json` mode the same rejection emits the machine envelope with a stable code instead of human text (RULE JCL) → Test `test_json_mode_emits_error_envelope`

### §2 — One exit code for a bad invocation

Every argument-shape rejection exits with the validation code, freeing exit 2 to mean network failure alone. **Implementation default:** map the whole `COMMANDER_USAGE_CODES` set to the validation code rather than enumerating a subset, because every member of that set is by definition an invocation error.

- **Dimension 2.1** [DONE] — commander usage rejections exit with the validation code → Test `test_usage_rejection_exit_code`
- **Dimension 2.2** [DONE] — a transport failure still exits with the network code → Test `test_network_failure_exit_code_unchanged`
- **Dimension 2.3** [DONE] — `--help` and a bare group node still exit 0 → Test `test_help_paths_exit_zero`

### §3 — Help lands on the stream that can be piped

A group node invoked bare currently writes its help to stderr while exiting 0, which is incoherent and breaks piping. It moves to stdout, matching `--help` and the bare root invocation. **Implementation default:** route through the same stdout path the root already uses, rather than special-casing each group.

- **Dimension 3.1** [DONE] — every group node invoked bare writes help to stdout at exit 0 → Test `test_group_node_help_on_stdout`
- **Dimension 3.2** [DONE] — every group node and every leaf accepts `--help`, exits 0, and names its required arguments → Test `test_help_names_required_arguments`

### §4 — Repair the handler-side messages that drifted

Three handlers already speak the house shape; one echoes its detail as its suggestion and one uses a terser form than its siblings. They converge on the `usage: agentsfleet …` wording. **Implementation default:** the usage line spells the full invocation including optional flags, matching `memory list`, because that is the version an operator can copy and run.

- **Dimension 4.1** [DONE] — `logs` rejected without an identifier suggests the usage line, not its own complaint → Test `test_logs_suggestion_names_the_fix`
- **Dimension 4.2** [DONE] — `grant list` adopts the same usage-line form → Test `test_grant_suggestion_form`

### §5 — A deterministic negative matrix that runs on every commit

Today `events` and `steer` prove their rejection only in the live acceptance lane, which needs a real API target and credentials, so `make test-unit-all` never sees them. The sweep moves into the deterministic lane and reads its command list from the existing matrix fixture. **Implementation default:** extend `command-matrix.ts` rather than starting a second list, because it already declares itself the single source of truth and a second list would drift.

- **Dimension 5.1** [DONE] — the matrix enumerates every command carrying a required positional or required option, `events` and `steer` included → Test `test_matrix_covers_every_required_arg_command`
- **Dimension 5.2** [DONE] — the sweep runs in the deterministic lane and needs no live target → Test `test_negative_sweep_is_deterministic`
- **Dimension 5.3** [DONE] — the matrix enumerates every group node for the help sweep → Test `test_matrix_covers_every_group_node`
- **Dimension 5.4** [DONE] — the connector list fixture carries a configured-but-not-connected row so the next-action text is asserted through the list renderer → Test `test_connector_list_renders_next_action`

## Interfaces

```
Rejected invocation — stderr, for every command:

  ✕ error: <what is wrong, in operator words>
    Suggestion: usage: agentsfleet <command> <required args> [optional flags]

Exit codes (cli/src/errors/index.ts EXIT_CODE, unchanged in shape):

  0  success, --help, bare group node, bare root
  1  auth failure, unexpected failure
  2  network / transport failure ONLY
  3  server answered with an error
  4  invocation rejected — missing or malformed argument, unknown command
  5  configuration failure
  130 operator interrupt

Help routing:

  agentsfleet                 -> help on stdout, exit 0
  agentsfleet <group>         -> group help on stdout, exit 0
  agentsfleet <group> --help  -> group help on stdout, exit 0
  agentsfleet <leaf> --help   -> leaf help on stdout, exit 0
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Missing required positional | `agentsfleet events` | `✕ error:` naming the argument + usage suggestion; exit 4; no network call |
| Missing required option | `agentsfleet schedule add <id>` without `--cron` | same shape naming the option; exit 4; no network call |
| Missing option value | `agentsfleet logs --fleet` | same shape naming the option that wants a value; exit 4; no network call |
| Malformed identifier | `agentsfleet logs --fleet not-a-uuid` | same shape naming the expected format with an example; exit 4; no network call |
| Unknown command | `agentsfleet pogo` | same shape carrying the did-you-mean text; exit 4 |
| Unknown subcommand on a group | `agentsfleet connector pogo` | same shape; exit 4; no network call, proven by an unroutable base URL |
| Excess arguments | `agentsfleet doctor extra` | same shape; exit 4 |
| Group node piped | `agentsfleet workspace \| cat` | help arrives on stdout; the pipe is non-empty; exit 0 |
| Daemon unreachable | valid invocation, dead API | network failure text; exit 2 — distinct from every row above |

## Invariants

1. Every command declaring a required positional or required option appears in the matrix fixture — enforced by a test that walks the built commander tree, collects required arguments, and diffs the set against `command-matrix.ts`, failing on any command the matrix omits.
2. No argument-shape rejection returns exit 2 — enforced by the sweep asserting the validation code on every matrix row, so a future handler that hand-rolls its own exit is caught.
3. A rejection's suggestion is never byte-identical to its detail — enforced by the sweep comparing the two rendered lines on every matrix row.
4. Every acceptance spec file belongs to exactly one lane — already enforced by `acceptance-lanes.test.ts`; the new file must be classified or that test fails.
5. No rejection path performs a network call — enforced by pointing the base URL at an unroutable address so a leaked fetch surfaces as a connection error instead of the expected stem.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | `command-instrumentation.ts` records `exit_code` as coarse `0 \| 1`, not the process exit code, so unifying process exit codes changes no emitted property | not applicable | unchanged — no new properties carry operator input | `test_usage_rejection_exit_code` asserts the process code; the existing telemetry unit tests assert the coarse property is untouched |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `test_missing_positional_uses_house_shape` | `events` with no identifier → stderr matches the `✕ error:` stem and a `Suggestion:` line; both present for every matrix row |
| 1.2 | e2e | `test_missing_option_uses_house_shape` | `logs --fleet` (no value) and `schedule add <id>` (no `--cron`) → same two lines |
| 1.3 | e2e | `test_unknown_command_keeps_suggestion` | `pogo` → house shape whose suggestion still carries the nearest command name |
| 1.4 | e2e | `test_suggestion_never_echoes_detail` | every matrix row → the suggestion line differs from the detail line |
| 1.5 | e2e | `test_json_mode_emits_error_envelope` | `--json events` → stderr parses as JSON carrying `error.code = MISSING_ARGUMENT`; `--json zzzz` → `UNKNOWN_COMMAND` naming the token |
| 2.1 | e2e | `test_usage_rejection_exit_code` | every matrix row → exit 4 |
| 2.2 | integration | `test_network_failure_exit_code_unchanged` | valid `list` against an unroutable base URL → exit 2, not 4 |
| 2.3 | e2e | `test_help_paths_exit_zero` | root, every group node, `--help` on each → exit 0 |
| 3.1 | e2e | `test_group_node_help_on_stdout` | each of the twelve group nodes invoked bare → stdout non-empty, stderr empty, exit 0 |
| 3.2 | e2e | `test_help_names_required_arguments` | `<leaf> --help` → exit 0 and the help body names each declared positional |
| 4.1 | unit | `test_logs_suggestion_names_the_fix` | `logs` with no identifier → detail names the missing identifier, suggestion carries the `usage: agentsfleet logs` line, and the two differ |
| 4.2 | e2e | `test_grant_suggestion_form` | `grant list` with no `--fleet` → suggestion carries the usage line |
| 5.1 | unit | `test_matrix_covers_every_required_arg_command` | tree walk over the built program → every required-argument command is present in the matrix; omission fails with the command name |
| 5.2 | unit | `test_negative_sweep_is_deterministic` | the new spec file appears in the deterministic lane list and not in the live list |
| 5.3 | unit | `test_matrix_covers_every_group_node` | tree walk → every command owning subcommands appears in the group-node table |
| 5.4 | integration | `test_connector_list_renders_next_action` | list response with a configured-but-not-connected provider → the rendered table carries the connect next-action text |
| regression | integration | `test_valid_invocations_unchanged` | `connector list`, `logs <id>`, `events <id>` against mock routes → exit 0 and unchanged output, proving the re-render touches only failure paths |
| regression | e2e | `test_help_bodies_unchanged` | `--help` bodies for root and each group → byte-identical to the pre-change snapshot |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every argument rejection shows the house shape (§1) | `bun ./cli/dist/bin/agentsfleet.js events 2>&1 \| head -2` | first line contains `✕ error:`, second contains `Suggestion:` | P0 | ✅ `✕ error: missing required argument 'fleet_id'` + `Suggestion:` line |
| R2 | Every argument rejection exits 4 (§2) | `for c in events steer stop kill; do bun ./cli/dist/bin/agentsfleet.js $c >/dev/null 2>&1; echo $?; done \| sort -u` | single line `4` | P0 | ✅ single line `4` across events/steer/stop/kill |
| R3 | Network failure stays exit 2 (§2) | `AGENTSFLEET_API_URL=http://127.0.0.1:1 bun ./cli/dist/bin/agentsfleet.js list >/dev/null 2>&1; echo $?` | `2` | P0 | ✅ `2` — network stays distinct |
| R4 | Group help is pipeable (§3) | `bun ./cli/dist/bin/agentsfleet.js workspace 2>/dev/null \| wc -c` | non-zero byte count | P0 | ✅ 555 bytes on stdout, stderr empty |
| R5 | The logs suggestion no longer echoes its detail (§4) | `bun ./cli/dist/bin/agentsfleet.js logs 2>&1 \| sort -u \| wc -l` | `2` | P1 | ✅ 2 distinct lines — the echo is gone |
| R5b | `--json` rejection is machine-parseable (§1) | `bun ./cli/dist/bin/agentsfleet.js --json events 2>&1 \| tail -6 \| python3 -c 'import json,sys; print(json.load(sys.stdin)["error"]["code"])'` | `MISSING_ARGUMENT` | P0 | ✅ `MISSING_ARGUMENT` |
| R6 | The negative sweep runs without a live target (§5) | `cd cli && bun test test/acceptance/argument-negatives.spec.ts` | exit 0 | P0 | ✅ 80 pass, 0 fail — no live target |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 20 paths, all in the table (table corrected at CHORE(close): `commander-boundary.unit.test.ts` and this spec were missing, the unedited `logs.integration.test.ts` row removed) |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `✓ All unit lanes passed`, exit 0 |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `make lint-all` exit 0 |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | N/A — no HTTP/schema/Redis surface in the diff |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ gitleaks: no leaks found |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output — largest changed source file is 326 lines |

**Test Delta (VERIFY):** Zig `unit=4157 integration=709` — identical to the CHORE(open) baseline, which is the expected reading for a TypeScript-only diff that adds no Zig. The growth this workstream is accountable for is the CLI package: 1533 → 1637 tests across 160 → 163 files (+104), with the 100% line-coverage floor holding.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the exit-2 branch of `exitFromCommanderError` | `grep -rn "COMMANDER_USAGE_CODES" cli/src/` | 2 matches, both in `lib/commander-boundary.ts` — its declaration and its single use |
| the argv-shape bare-group resolver replaced at REVIEW | `grep -rn "resolveBareGroup\|installGroupHelpActions" cli/src/` | 0 matches |

## Out of Scope

- Renaming `--cursor` to `--starting-after` on `logs` and `events` — a separate surface change already noted in `cli-tree-fleet.ts`.
- Reordering the workspace-context guard ahead of argument validation, so a missing argument reports before "no workspace selected". Recorded as Discovery in `help-and-errors.spec.ts`; it changes the auth path and earns its own workstream.
- Server-side `UZ-*` error text. This workstream touches only client-side rejection.
- Moving the remaining live-lane acceptance specs into the deterministic lane; only the argument-negative rows move here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator types `agentsfleet events`, forgets the identifier, and the terminal tells them exactly what to type next: `usage: agentsfleet events <fleet_id>`. They fix it on the first try without opening docs or `--help`.
2. **Preserved user behaviour** — every successful invocation prints byte-identical output; every `--help` body is unchanged; the `<required>` declarations and command names stay put. Only failure text, failure exit codes, and the stream group help lands on change.
3. **Optimal-way check** — the direct route is one re-render at the output boundary plus one exit mapping. The unconstrained-optimal shape would have each handler own its own rejection with per-argument guidance, but that is twenty-five handler edits for a marginal wording gain and it re-opens handler purity. The gap is accepted: the central hook gets one shape everywhere in one place.
4. **Rebuild-vs-iterate** — iterate. Determinism improves rather than degrades: the sweep moves from a credentialed live lane into the deterministic one.
5. **What we build** — an `outputError` re-render, a usage-code exit mapping, a stdout route for bare group help, two handler message repairs, an extended matrix fixture, and one deterministic acceptance spec.
6. **What we do NOT build** — per-argument bespoke guidance, a new error registry code, a machine-readable rejection envelope for `--json`, or any change to the guard ordering that makes workspace context report before argument shape.
7. **Fit with existing features** — compounds with the existing `CliError` taxonomy, which already declared validation as 4; this makes the parse path honour a decision the code had only half-applied. The feature it must not destabilize is the auth guard, which runs in the same dispatch path and is asserted by the live login handshake.
8. **Surface order** — CLI-first, the repository default, and CLI-only: no dashboard surface exists for invocation errors.
9. **Dashboard restraint** — N/A — no user interface surface.
10. **Confused-user next step** — the suggestion line is itself the self-serve move: it prints a runnable command. Where the missing value must be discovered rather than typed, the usage line points at the command that lists it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections split by observable outcome — shape, exit code, help stream, message repair, coverage — because each grades independently in the rubric and each can regress independently.
- **Alternatives considered:** (a) convert every `<required>` positional to optional and validate inside each handler, giving one code path but touching twenty-five handlers, losing commander's automatic usage text, and risking silent behaviour drift per command — rejected as disproportionate; (b) leave exit codes alone and unify only the rendered text — rejected because it preserves the exit-2 overload that makes the failure unscriptable, which is the defect with the sharpest operator cost.
- **Patch-vs-refactor verdict:** this is a **patch** because the taxonomy, the tree walk, and the house error shape all already exist; the diff makes one dispatch path honour them. No follow-up refactor is implied beyond the guard-ordering item already recorded in Out of Scope.

## Discovery (consult log)

- **Consults** — Indy chose the exit-code unification target on Aug 19, 2026: usage rejection unifies on the validation code 4, leaving exit 2 to mean network failure alone, over the alternatives of unifying on 2 or leaving exit codes untouched.
- **Metrics review** — no analytics or funnel playbook update required: `command-instrumentation.ts` records a coarse `0 | 1` outcome, not the process exit code, so no emitted property changes.
- **Skill-chain outcomes** — `/write-unit-test`: the package's own 100% coverage floor is the audit; it failed the first run on three uncovered lines (`cli.ts` bare-group branch, `commander-boundary.ts` writeErr) because only the subprocess acceptance spec reached them, and `commander-boundary.unit.test.ts` was added to cover them in-process. Floor now PASS at 100.00%. REVIEW: run inline rather than through the gstack specialist fan-out, since this session's rules bar subagent dispatch; it caught one real defect, recorded below. `kishore-babysit-prs` runs after the push.
- **Review finding (fixed in the same branch)** — the first bare-group implementation resolved the case from the argv shape, so `agentsfleet workspace` printed help on stdout while `agentsfleet workspace --json` still printed it on stderr: the very inconsistency this workstream removes, one flag away. Root cause is commander answering an action-less group with `help({ error: true })`. Fixed by overriding `help` on those nodes to force `error: false`, which holds for every invocation shape. Two earlier approaches were tried and rejected with evidence: filtering the write stream splits commander's `addHelpText` tail onto the wrong stream (caught by the spec's bare-vs-`--help` equality row), and attaching an action to the group makes an unknown subcommand report as an excess argument instead of an unknown command. A regression row now asserts group help reaches stdout with a global flag present.
- **Environment** — `make test-unit-all` first failed in `_ensure-test-infra`: the postgres test container died with `initdb: could not create directory ... No space left on device`, with Docker holding ~7 GB of reclaimable build cache. Indy approved a prune; the datastores came up clean afterwards. Unrelated to this diff.
- **Flaky lane observed** — `ui/packages/app` failed one dialog-timing test per full-lane run, a different test each time (`WorkspaceSwitcher > opens the create dialog`, then `FleetLibrariesView > prefills the dialog ...`), and passed 2388/2388 on re-run in the same worktree with the same code. The branch touches no `ui/` file. Recorded as a pre-existing flake, not a finding of this workstream.
- **Metrics review** — no analytics or funnel playbook update required: `command-instrumentation.ts` records a coarse `0 | 1` outcome, not the process exit code, so no emitted property changes.
- **Deferrals** — none recorded.
