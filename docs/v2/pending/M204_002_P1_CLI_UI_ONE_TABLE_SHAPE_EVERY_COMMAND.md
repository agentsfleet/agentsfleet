<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M204_002: Every command prints one table shape, and every global flag we advertise does something

**Prototype:** v2.0.0
**Milestone:** M204
**Workstream:** 002
**Date:** Sep 21, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: every table's columns change, and two advertised flags change behaviour.
**Categories:** CLI, UI
**Batch:** B1 — single stream; the helper precedes the call sites, the call sites precede the golden fixture.
**Branch:** pending — set at CHORE(open)
**Folded-into:** `M204_001` — one branch, one Pull Request; that spec is the sole non-folded owner.
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** `M204_001` — it adds `library list`, whose table this workstream reshapes; both land on one branch and one Pull Request.
**Provenance:** agent-generated (pre-spec, a command sweep of the built binary, Sep 21, 2026)
**Canonical architecture:** `docs/architecture/web_app.md` §Dashboard surfaces

---

## Overview

**Goal (testable):** Every table the command-line interface (CLI) prints renders name, then its entity identifier, then its domain columns, then `AGO`, from one helper; `--wizard` is unadvertised; `--completions` and `--log-level` are covered by tests that fail when they stop working; and `status <fleet_id>` reports one fleet.

**Problem:** An operator reading two `agentsfleet` tables reads two layouts. `api-key list` puts its identifier last, `library` and `schedule list` put theirs first, `workspace list` opens with a bare `*`. None of them says how old anything is, so "which of these did I make this morning" is unanswerable from the output — the question an operator actually arrives with. Two of the flags every help page advertises do nothing an operator can observe: `--wizard` opens a builder nobody designed, and `--log-level` validates a level it then never applies. `status` takes no fleet identifier, so the one-fleet question has no one-fleet command.

**Solution summary:** One helper in the output layer owns column order and appends the age column itself, and all thirteen table call sites pass their columns through it, so a table cannot omit age or reorder itself by being edited. The age reads `created_at`, which every relevant payload already carries on the wire, so no endpoint changes. `--log-level` is wired to the HTTP client so the level governs records that exist; `--wizard` stops being advertised in the help this repository renders; `--completions` gains the test that proves the emitted script parses. `status` accepts an optional fleet identifier and keeps its workspace-wide answer when given none.

## PR Intent & comprehension handshake

- **PR title (eventual):** `refactor(cli,app): one table shape, and flags that do what the help says`
- **Intent (one sentence):** An operator who has read one `agentsfleet` table can read every other one, knows how old each row is, and is never offered a flag that does nothing.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/src/commands/fleet_list.ts` — the column order every other table is moving to: name, entity identifier, state. It is the reference because it is already right, not because it is first.
2. `cli/src/output/format.ts` — `formatTable` and `cell`, the layer the new helper joins; `EMPTY_CELL` is the value a missing age renders.
3. `cli/src/lib/http-retry.ts` — owns the retry decision `--log-level debug` must make observable; read it before adding a record, so the record names the decision the code already took.
4. `cli/src/program/entry/help-formatter.ts` — this repository renders its own help, which is why `--wizard` can be unadvertised without touching the Effect CLI built-in behind it.
5. `dispatch/write_ts_adhere_bun.md` — the TypeScript file-shape decision and the named-constant discipline both surfaces in this diff trip.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/output/format.ts` · `cli/src/output/index.ts` (EDIT) | EDIT | The age formatter and the column helper every table passes through. `format.ts` already owns `formatTable`, `cell` and `EMPTY_CELL`, so the order rule lands beside the renderer rather than in a new module nothing else imports. |
| `cli/src/commands/{fleet_list,fleet_library,fleet_library_list,workspace,api_key,fleet_schedule,fleet_secret_list,memory,billing,connector,approvals,grant,tenant}.ts` (EDIT) | EDIT | Thirteen table call sites trade a hand-written column array for the helper. `fleet.ts` and `fleet_install_source.ts` widen their row interfaces to carry `created_at`, which the wire already sends and these types drop. |
| `cli/src/commands/fleet.ts` · `cli/src/commands/fleet_install_source.ts` · `cli/src/program/tree/fleet.command.ts` (EDIT) | EDIT | The two row interfaces gain `created_at`; the command tree gives `status` its optional fleet-identifier argument beside the existing lifecycle verbs. |
| `cli/src/lib/http-retry.ts` · `cli/src/services/http-client.ts` (EDIT) | EDIT | The request line, the attempt ordinal and the retry verdict become log records, so a level has something to govern. The retry decision is already taken here; this names it. |
| `cli/src/program/entry/help-formatter.ts` (EDIT) · `cli/test/golden/help-no-color.txt` (EDIT) | EDIT | The help stops listing `--wizard`, which moves the golden fixture that pins the help byte-for-byte. |
| `cli/test/{table-column-order,ago-format,completions,log-level,status-argument}.unit.test.ts` (CREATE) | CREATE | One test file per Dimension family, so a failure names which claim broke. |
| `cli/test/acceptance/fixtures/command-matrix.ts` · `cli/test/acceptance/lifecycle-with-token.spec.ts` (EDIT) | EDIT | Three fixture claims describe a looser CLI than this repository ships, and two of them suppress coverage by excluding rows from the no-network sweep. Corrected with the `status` row, whose positional this workstream adds. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallSourceSelector.tsx` (EDIT) · `ui/packages/app/tests/install-source-tier.test.ts` (CREATE) | EDIT / CREATE | The install picker names each entry's tier. It reads `visibility` today only as a React key, so the field is already in hand and nothing new is fetched. |
| `cli/src/program/tree/schedule.command.ts` · `cli/src/commands/fleet_schedule.ts` · `cli/src/cli.ts` · `cli/test/fleet-schedule.unit.test.ts` · `cli/test/fleet-schedule.integration.test.ts` (EDIT) | EDIT | §6's three renamed verbs and the vendor name dropped from the `sync` description. The two test files name the old verbs and move with them. |
| `docs/architecture/web_app.md` (EDIT) | EDIT | Records that the CLI has one table shape and that the install picker distinguishes the two library tiers. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every column label, level name and age unit is a named constant, declared once); **NDC** (the helper ships with all thirteen call sites calling it — a helper with one caller is the dead code this rule names); **NLR** (the three stale fixture claims are in files this diff already opens, so they are fixed rather than left); **NLG** (no compatibility spelling is kept for the old column orders and no flag alias is added); **ORP** (nothing is renamed or deleted — the sweep is the `N/A` assertion below); **TSJ**/**TSC** (every touched TypeScript file); **UIS**/**DTK** (the install picker); **MSID** (no milestone marker reaches source).
- **`dispatch/write_ts_adhere_bun.md`** — the TypeScript FILE SHAPE DECISION at PLAN for `format.ts`, which gains two exports; `const` and import discipline across thirteen command files; design-system primitives and token utilities for the picker's tier label.
- **`docs/DOCUMENTATION_RULES.md`** and **`docs/CHANGELOG_VOICE.md`** — §5's prose, the changelog entry and the four `~/Projects/docs` pages a public output change moves.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — column labels, nine level names, age units | Each string a named `const` in `cli/src/constants/`, imported by the helper and the call sites rather than respelled at each table. |
| UI GATE · DESIGN TOKEN GATE | yes — the install picker gains a tier label | A design-system primitive for the label and token utilities only, mirroring the badge the Models registry table already uses for its tier. `bash audits/design-tokens.sh` before COMMIT. |
| File & Function Length (≤350/≤50/≤70) | yes — `format.ts` and thirteen command files | `format.ts` is measured with `wc -l` before the helper lands; if the age formatter and the column helper together approach the cap, the age formatter moves to `cli/src/output/age.ts` and `format.ts` re-exports it. |
| LENGTH GATE · PUB / FILE SHAPE | yes — every touched source file; two new exports | The two exports are declared at PLAN as operations-over-value: the column helper and the age formatter, and nothing else leaves the module. |
| GREPTILE GATE · Architecture consult | yes — every EXECUTE turn; the CLI's output shape becomes a stated convention | Read the rule-code gloss legend plus the sections Applicable Rules names, per `docs/EXECUTE_DOC_READS.md`; read `docs/architecture/web_app.md` before the picker changes and update it in the same Pull Request. |
| SCHEMA GUARD · ERROR REGISTRY · ZIG GATE | no | No `schema/*.sql`, no `*.zig`, and no new error code: `status`'s malformed identifier reuses the `INVALID_ARGUMENT` rejection `validateRequiredId` already emits. `bash audits/error-codes.sh` stays green, which is what proves it. |

## Prior-Art / Reference Implementations

- **Reference (CLI):** the "7 Pillars" of command-line developer experience per `docs/TEMPLATE.md` Prior-Art, read against `cli/src/output/format.ts`. Aligned: output as a service — the renderer decides layout, and after this diff it also decides order, so a handler cannot express a column order at all. Aligned: structured errors keep their envelope; `status`'s malformed identifier answers `INVALID_ARGUMENT` like every other identifier-taking verb. Divergence: none — this workstream moves the CLI toward the pillar it was already closest to.
- **Reference (the tier label):** `ModelsRegistryTable.tsx` renders a platform-versus-tenant distinction for the models domain already. The install picker takes its label shape from there rather than inventing a second vocabulary for the same two-tier idea, which is the rule M204_001 §0b established for this pair of tiers.
- **Reference (the age column):** `cli/src/commands/memory.ts` already prints an `UPDATED` column from a server timestamp, so the repository has a timestamp-to-column precedent; what it lacks is one relative formatter the rest can share.

## Sections (implementation slices)

### §1 — One table shape, owned by one helper

The column order and the age column, together, because separating them would land two conventions in one file and leave the second to drift. **Implementation default:** the helper takes name, identifier and domain columns and appends `AGO` itself, rather than each call site listing `AGO` last, because thirteen arrays that agree today are the shape that disagrees after the next edit — and `api_key.ts` already proves it, having drifted its identifier to last position while three siblings drifted theirs to first. **Implementation default:** the age renders from `created_at`, already on the wire at `rustd/crates/afd_wire/src/fleet.rs:108` and `:177` and `workspace_library.rs:56`, and already decoded at `cli/src/commands/workspace-response-decoders.ts:8`; no endpoint changes and no read widens. **Implementation default:** `workspace list`'s `ACTIVE` column becomes `STATUS` carrying the word `active`, because the requested header is `STATUS` and rendering the marker as a word keeps the signal the bare `*` carried instead of dropping it. **Implementation default:** `secret list`'s and `api-key list`'s `CREATED` become `AGO`; `api-key list` keeps `LAST_USED` as its own column, because last use and age answer different questions.

- **Dimension 1.1** — Every entity table renders name, then its identifier, then its domain columns, then `AGO`, and the order comes from one helper rather than from each call site → Test `test_every_entity_table_shares_one_column_order`
- **Dimension 1.2** — An age renders for each magnitude from seconds to years, a missing or unparseable timestamp renders the empty cell rather than a wrong number, and a timestamp in the future does not render a negative age → Test `test_ago_renders_every_magnitude_and_refuses_to_invent_one`
- **Dimension 1.3** — A table whose payload carries no timestamp still renders `AGO`, holding the empty cell, so absence reads as absence rather than as a column nobody added → Test `test_a_table_without_timestamps_still_renders_the_column`
- **Dimension 1.4** — No command file expresses a column order any more: the labels live with the helper and the call sites name only their columns → Test `test_no_call_site_declares_its_own_column_order`
- **Dimension 1.5** — Machine-readable output does not move: the reshape is a text-renderer change, so every `--json` answer is byte-identical to the pre-diff one → Test `test_json_output_is_unchanged_by_the_table_reshape`

### §2 — The global flags we advertise do something

Three flags, one rule: a flag in the help does what the help says, or it is not in the help. **Implementation default:** `--log-level` is wired rather than hidden, because `cli/src` holds zero `Effect.log*` call sites today, so the level governs nothing — and a client that dials a server and carries a retry policy with no way to watch either is worth a record more than it is worth a deletion. **Implementation default:** `--wizard` is unadvertised rather than deleted: it is an Effect CLI built-in this repository never designed, its first prompt asks an operator to set the API base URL, and hiding it from the help this repository renders is a change to our own formatter where removing the built-in is not ours to make.

- **Dimension 2.1** — `--log-level debug` makes the request line, the attempt ordinal and the retry verdict observable on the error stream, and `--log-level none` emits none of them, so the flag governs records that exist → Test `test_log_level_governs_records_that_exist`
- **Dimension 2.2** — The rendered help does not advertise `--wizard`, and the flag still builds a command for a caller who names it → Test `test_help_does_not_advertise_the_undesigned_flag`
- **Dimension 2.3** — `--completions` emits a script the target shell parses for each of the four shells it offers, and an unoffered shell is refused naming the four → Test `test_completions_emit_a_script_each_shell_parses`

### §3 — One fleet has a one-fleet command

`status` answers for the workspace and cannot answer for a fleet, so the narrowest question needs the widest command. **Implementation default:** the identifier is optional and the bare command keeps its workspace-wide answer, because changing what bare `status` means would change a shipped command for every caller — the rule M204_001 §5 applied to its own bare verb. **Implementation default:** the identifier runs `validateRequiredId` like every other identifier-taking verb, so a malformed one is refused before a request is issued.

- **Dimension 3.1** — `status <fleet_id>` reports that one fleet, bare `status` keeps reporting every fleet in the active workspace, and a malformed identifier is refused client-side without a request → Test `test_status_reports_one_fleet_or_the_whole_workspace`

### §4 — The install picker names the tier

The one dashboard surface this workstream touches, because it is where an operator first meets the two library tiers and cannot currently tell them apart. Two entries of the same name, one platform and one this workspace onboarded, render identically. **Implementation default:** the label reads `visibility`, which the component already holds — it passes the field to React as part of a key at `InstallSourceSelector.tsx:187` and renders nothing from it — so nothing new is fetched. **Implementation default:** the label takes its shape from `ModelsRegistryTable.tsx`, which already distinguishes these two tiers for the models domain; a second vocabulary for one idea is what M204_001 §0b rejected.

- **Dimension 4.1** — The install picker names the tier an entry came from, so a platform default and a workspace's own copy of the same name are distinguishable by rendered text alone → Test `test_the_install_picker_names_each_entry_tier`

### §5 — The fixture claims that describe a looser CLI than we ship

Three claims in the acceptance fixtures are measurably false, and two of them cost coverage: a row marked as not validating client-side is excluded from the sweep that proves no network call fires, so real validation goes unasserted. **Implementation default:** the claims are corrected in place rather than deleted, because each one carries a reason a future reader needs; what changes is the fact, not the commentary. RULE NLR applies — these files are already open in this diff.

- **Dimension 5.1** — Every identifier-taking verb that rejects a malformed identifier client-side is marked as doing so, and the sweep that proves no network call fires covers each of them → Test `test_the_client_side_sweep_covers_every_validating_verb`
- **Dimension 5.2** — No fixture comment cites a path that does not resolve, and the error codes the matrix expects are cited where they are declared → Test `test_fixture_comments_cite_paths_that_resolve`

### §6 — The schedule verbs say what the rest of the surface says

`schedule` speaks a private dialect: `add` where every other collection says `create`, `rm` where the rest says `delete`, and `status` where a single-resource read is called `show` — and `status` is doubly wrong, because the top-level `status` is a workspace read while this one takes two identifiers. Its `sync` description also names a scheduling vendor this platform no longer runs on. **Implementation default:** no alias is kept for any retired spelling; RULE NLG forbids a compatibility verb before `0.30.0`, and the retired spelling answers as an unknown subcommand pointing at the group's list, which is the refusal `argument-negatives.spec.ts` already pins for that shape. **Implementation default:** the `sync` description loses the vendor name rather than swapping in the current one, because the verb re-applies a schedule and which host receives it is not a caller's concern.

- **Dimension 6.1** — `schedule create`, `schedule delete` and `schedule show` are the only spellings the tree accepts, each behaving exactly as the verb it replaces did, and each retired spelling is refused as an unknown subcommand naming the group's list → Test `test_schedule_verbs_match_the_rest_of_the_surface`
- **Dimension 6.2** — No command description in the tree names a scheduling vendor → Test `test_no_command_description_names_a_vendor`

### §7 — Documentation

The public output changes, so the docs branch is part of the work. **Implementation default:** `~/Projects/docs` work happens on its own branch `chore/m204-one-table-shape-changelog`, cut from `main` there and never edited through this worktree.

- **Dimension 7.1** — The architecture doc records the one table shape and the picker's tier label, and the diff carries one new changelog entry → Test `test_architecture_and_changelog_record_the_table_shape`
- **Dimension 7.2** — The `~/Projects/docs` pages showing CLI table output are revised on their own branch → Test `test_docs_branch_carries_the_revised_pages`

## Interfaces

```
cli/src/output/format.ts — the locked surface:

  entityTable(spec: EntityTableSpec, rows: ReadonlyArray<TableRow>): string
    EntityTableSpec = {
      name:   TableColumn                      // rendered first
      id?:    TableColumn                      // rendered second when present
      domain: ReadonlyArray<TableColumn>       // rendered in the given order
    }
    AGO is appended by the helper. A caller cannot place it, omit it, or
    reorder the three groups. A spec naming AGO in `domain` is a type error.

  ago(createdAtMs: unknown, nowMs?: number): string
    integer past instant  -> "45s" | "12m" | "3h" | "9d" | "2y"
    undefined | null | NaN | non-integer | future instant -> EMPTY_CELL

cli/src/program/tree/fleet.command.ts — status gains one optional argument:

  agentsfleet status [fleet_id] [flags]
    fleet_id present  -> that fleet's row alone
    fleet_id absent   -> every fleet in the active workspace (unchanged)
    fleet_id malformed-> INVALID_ARGUMENT, non-zero exit, no request issued
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Missing timestamp | A payload omits `created_at`, or a server sends `null` | `ago` returns the empty cell; the row renders with every other column intact and the table's alignment is unchanged. |
| Future timestamp | Client and server clocks disagree | `ago` returns the empty cell rather than a negative age; no error and no throw. |
| Unparseable timestamp | A non-integer or out-of-range value reaches the formatter | The empty cell, by the same path as missing — the formatter validates rather than trusting its input. |
| Malformed fleet identifier | `status not-a-uuid` | `INVALID_ARGUMENT` with the uuidv7 example, non-zero exit, and no request issued — asserted against an unroutable API URL. |
| Unoffered completion shell | `--completions powershell` | Non-zero exit naming the four shells, in the existing rejection envelope. |
| Log records with no level | A record is emitted where no level governs it | Not reachable by construction: the records are added behind the level the flag sets, and the default level emits none of them, which Dimension 2.1 asserts in both directions. |

## Invariants

1. **A call site cannot express a column order** — enforced by the type: `entityTable` takes name, identifier and domain separately, so there is no position for a caller to choose, and `AGO` is appended by the helper. Dimension 1.4 asserts no command file declares an order.
2. **`AGO` is last in every table** — enforced by construction, since the helper appends it after the domain columns and a spec naming it in `domain` does not type-check.
3. **An age is never invented** — enforced at runtime: `ago` validates its input and returns `EMPTY_CELL` for anything not an integer instant in the past. Dimension 1.2 asserts each rejected shape.
4. **A malformed identifier never reaches the network** — enforced by `validateRequiredId` running before dispatch, asserted against an unroutable API URL so a pass proves no call fired.
5. **An advertised flag does something observable** — enforced by test, not by discipline: Dimension 2.1 asserts both directions of `--log-level`, 2.2 pins the help byte-for-byte against the golden fixture, and 2.3 parses each emitted completion script.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `cli.http.attempt` | ops | A request is issued or retried, and only at `debug` or finer | method, path template, attempt ordinal, retry verdict, status class | No token, no Authorization header, no request or response body, no secret name or value; the path is the template, never an interpolated identifier | `test_log_level_governs_records_that_exist` |

No product analytics event is added: reshaping a table and covering two flags changes no funnel step, so no analytics playbook update is required. The one record above is operator diagnostics, emitted only when an operator asks for it.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_every_entity_table_shares_one_column_order` | For each of the thirteen tables the rendered header is name, identifier, domain columns, `AGO` — `list` → `NAME FLEET STATUS AGO`; `library` → `NAME LIBRARY TIER SECRETS AGO`; `workspace list` → `NAME WORKSPACE STATUS AGO`; `api-key list` → `NAME API_KEY_ID STATUS LAST_USED AGO`; `secret list` → `NAME KIND AGO`. |
| 1.2 | unit | `test_ago_renders_every_magnitude_and_refuses_to_invent_one` | `now-45s` → `45s`; `now-90m` → `1h`; `now-36h` → `1d`; `now-400d` → `1y`; `undefined`, `null`, `NaN`, `"x"` and a non-integer → `EMPTY_CELL`; `now+60s` → `EMPTY_CELL`, never `-1m`. |
| 1.3 | unit | `test_a_table_without_timestamps_still_renders_the_column` | A `connector list` fixture carrying no timestamp → the header ends `AGO` and every body row holds `EMPTY_CELL` in it; column alignment matches the same table rendered with timestamps present. |
| 1.4 | unit | `test_no_call_site_declares_its_own_column_order` | `grep -rn 'label: "' cli/src/commands/` → 0 matches; every table in `cli/src/commands/` reaches the renderer through `entityTable`. |
| 2.1 | unit | `test_log_level_governs_records_that_exist` | With a stub transport and one injected retry: `--log-level debug` → stderr carries the method, the path template, `attempt=1`, `attempt=2` and the retry verdict; `--log-level none` and the default → stderr carries none of the five; no record contains the bearer token; `--log-level bogus` → non-zero exit naming the nine levels. |
| 2.2 | unit | `test_help_does_not_advertise_the_undesigned_flag` | `--help` stdout contains no `--wizard`; the golden fixture matches byte-for-byte; `agentsfleet --wizard` with closed standard input still opens the builder and exits 0. |
| 2.3 | unit | `test_completions_emit_a_script_each_shell_parses` | `--completions bash` → non-empty stdout that `bash -n` accepts; `zsh` likewise under `zsh -n`; `fish` and `sh` each emit non-empty stdout; `--completions bogusshell` → non-zero exit naming all four. |
| 3.1 | unit | `test_status_reports_one_fleet_or_the_whole_workspace` | `status <uuidv7 of a seeded fleet>` → that fleet's row alone; bare `status` → every fleet in the active workspace, byte-identical to the pre-diff output over the same fixture; `status not-a-uuid` → non-zero exit, `INVALID_ARGUMENT`, and no request issued against an unroutable API URL. |
| 3.1 | e2e | `test_status_by_identifier_subprocess_walk` | The real binary in a subprocess against the acceptance stack: install a fleet, `status <its id>` names it and no other, `status` names it among the workspace's fleets. |
| 5.1 | unit | `test_the_client_side_sweep_covers_every_validating_verb` | Every `REQUIRES_IDENTIFIER` row whose verb rejects `not-a-uuid` without dialling carries `validatesClient: true` — `stop`, `kill`, `resume`, `logs`, `api-key delete`, `workspace use`, `workspace delete`; and the sweep runs each against an unroutable API URL with no `ECONNREFUSED` observed. |
| 5.2 | unit | `test_fixture_comments_cite_paths_that_resolve` | Every repository path cited in a comment in `command-matrix.ts` and `lifecycle-with-token.spec.ts` satisfies `test -f`; the three `UZ-*` codes the matrix expects are each found by `grep` in `rustd/crates/afd_core/src/error_code/`. |
| 6.1 | unit | `test_schedule_verbs_match_the_rest_of_the_surface` | `schedule create <fleet> --cron '0 9 * * *'` behaves as `schedule add` did over the same fixture; `schedule delete <fleet> <id>` as `rm` did; `schedule show <fleet> <id>` as `status` did; `schedule add`, `schedule rm` and `schedule status` each exit 4 naming the unrecognised token and pointing at `agentsfleet schedule --help`. |
| 6.2 | unit | `test_no_command_description_names_a_vendor` | Every `Command.withDescription` string in `cli/src/program/tree/` matched against the vendor names this platform has used → 0 matches; `schedule sync --help` reads `Re-apply a hosted schedule`. |
| 7.1 | unit | `test_architecture_and_changelog_record_the_table_shape` | `docs/architecture/web_app.md` contains the one-table-shape rule and the picker's tier label; the diff against the comparison revision adds exactly one new `<Update>` block. |
| 7.2 | manual | `test_docs_branch_carries_the_revised_pages` | Procedure: in `~/Projects/docs` on `chore/m204-one-table-shape-changelog`, `git diff --name-only main` lists the CLI reference page and `changelog.mdx`. Required person: Indy, who owns that repository's branch. Evidence: the branch name and command output in Pull Request Session Notes. |
| 1.5 | unit | `test_json_output_is_unchanged_by_the_table_reshape` | For `list`, `library`, `workspace list`, `secret list` and `api-key list`, `--json` output over a fixed fixture is byte-identical to the pre-diff output: the reshape is a text-renderer change and no machine-readable consumer moves. |
| 4.1 | unit | `test_the_install_picker_names_each_entry_tier` | A gallery fixture holding one `platform` and one `tenant` entry of the SAME name → two cards, each naming its own tier, distinguishable by rendered text alone. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every table shares one column order and carries age (§1) | `cd cli && bun test test/table-column-order.unit.test.ts test/ago-format.unit.test.ts` | exit 0 | P1 | |
| R2 | No table's columns are written at the call site (§1) | `grep -rn 'label: "' cli/src/commands/ \| wc -l` | `0` | P1 | |
| R3 | Machine-readable output did not move (§1.5) | `cd cli && bun test test/json-output-regression.unit.test.ts` | exit 0 | P0 | |
| R4 | The flags we advertise do something (§2) | `cd cli && bun test test/log-level.unit.test.ts test/completions.unit.test.ts && ./dist/bin/agentsfleet.js --help \| grep -c wizard` | exit 0, then `0` | P1 | |
| R5 | Completions emit a script the shell parses (§2) | `cd cli && ./dist/bin/agentsfleet.js --completions bash \| bash -n && ./dist/bin/agentsfleet.js --completions zsh \| zsh -n` | exit 0 twice | P1 | |
| R6 | One fleet has a one-fleet command (§3) | `cd cli && bun test test/status-argument.unit.test.ts` | exit 0 | P1 | |
| R7 | The acceptance fixtures describe the CLI we ship (§5) | `cd cli && bun test test/acceptance/argument-negatives.spec.ts && grep -c 'error_registry.zig' test/acceptance/fixtures/command-matrix.ts` | exit 0, then `0` | P1 | |
| R8 | The install picker distinguishes the two tiers (§4) | `cd ui/packages/app && bun run test -- install-source-tier` | exit 0 | P1 | |
| R9 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R10 | The schedule verbs match the rest of the surface (§6) | `cd cli && bun test test/fleet-schedule.unit.test.ts && ./dist/bin/agentsfleet.js schedule --help \| grep -cE '^  (add|rm|status) '` | exit 0, then `0` | P1 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration lane green (live Postgres and Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** every declared `conform` and `verify.*` invocation from `.oracle/orly.json` appears verbatim above with an Expected value. **Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows point at the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 whose scope moves whole into a named successor spec is marked `MOVED to M{N}_{NNN} R{n}` — never ✅ — and only when that spec exists carrying the criterion as its own P0, both specs record the mapping, and Discovery holds the owner's verbatim quote authorising it.

## Dead Code Sweep

N/A — no files deleted. Nothing is renamed or removed: the helper is a new export the thirteen call sites adopt, the age column is additive, `status`'s argument is optional, `--wizard` keeps working and only stops being advertised, and the three fixture corrections change facts inside comments and one boolean field rather than removing either. The one thing superseded is each call site's own column array, which this diff replaces in place; Dimension 1.4's grep is what proves none survived.

## Out of Scope

- **Renaming `library remove`, or any verb outside `schedule`.** §6 renames the three `schedule` verbs Indy named and nothing else. `library remove` versus `library rm` is the same class of inconsistency and is left alone here, because renaming a verb M204_001 is shipping in the same Pull Request would change a command twice in one release.
- **A `library` column showing how many fleets were installed from each entry.** Asked about at authoring and left unresolved; it needs a count the gallery read does not return, so it is a server change, not a column.
- **The dashboard's own table layouts.** This workstream standardises the CLI and adds one label to the install picker. The dashboard's lists follow the design system, not the CLI's column rule, and aligning them is a design decision with no operator complaint behind it yet.
- **Making `--wizard` a designed surface.** Hiding it is the honest minimum. A wizard worth advertising asks about the command being built, not about the API base URL first, and that is a product design task.

## Product Clarity (authoring record)

1. **Successful user moment** — An operator runs `agentsfleet library`, sees `2h` beside one entry and `41d` beside another, and deletes the one they made this morning by mistake without opening the dashboard to work out which it was.
2. **Preserved user behaviour** — Every command keeps its name, its flags and its exit codes; `--json` output is byte-identical, which a regression row asserts; bare `status` and bare `library` keep their current answers. Only the text table's columns move.
3. **Optimal-way check** — The most direct route to that moment is an age column, and the wire already carries the timestamp, so nothing is fetched to get it. The gap to the unconstrained-optimal shape is that ages are relative and coarse — `2h` does not say which two hours — and that is acceptable because the absolute instant is one `--json` away for anyone who needs it.
4. **Rebuild-vs-iterate** — Iterate. The helper is the refactor: it moves order out of thirteen call sites into one, which is the change that keeps this from re-drifting. A larger rebuild of the output layer was rejected — `formatTable` renders correctly today and its callers are not the problem, their column arrays are.
5. **What we build** — One column helper, one age formatter, thirteen call-site edits, one optional `status` argument, three log records behind an existing flag, one hidden flag, one tier label, and the tests that pin each.
6. **What we do NOT build** — No new endpoint, no new error code, no verb renames, no dashboard table changes, no wizard redesign, no absolute-timestamp column beside the relative one.
7. **Fit with existing features** — It compounds with M204_001: that workstream gives a workspace its own library collection and a removal verb, and this one makes the resulting list readable enough to decide what to remove. The feature it must not destabilise is `--json`, which every script and the acceptance suite parse.
8. **Surface order** — CLI-first, the repository default, with one UI label riding along because the same two-tier distinction is invisible in the install picker and an operator meets it there first.
9. **Dashboard restraint** — The picker gains a tier label and nothing else. No filter, no sort, no tier toggle: the label is the evidence, and a control before anyone has asked to filter by tier would be a control before evidence.
10. **Confused-user next step** — `agentsfleet <command> --help` names every column's meaning, and `--json` gives the absolute instant behind any `AGO`. An operator who cannot tell two same-named library entries apart now reads the tier in both surfaces.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Five implementation Sections plus documentation, split by what each proves rather than by file: the table shape, the advertised flags, the missing `status` argument, and the fixture claims that describe a looser CLI than this repository ships. §1 is the only Section with cross-file reach, which is why the helper lands before any call site moves.
- **Alternatives considered:** (a) A per-call-site edit adding `AGO` and reordering by hand — rejected: it produces the same output today and the same drift tomorrow, and `api_key.ts` is the proof that hand-maintained column arrays diverge. (b) Its own milestone, M206, separate branch and Pull Request — rejected by owner direction; the work rides M204's branch and Pull Request as a folded workstream. (c) Hiding `--log-level` beside `--wizard` — rejected: the retry decision it would expose is the one an operator needs when a command is slow, and `http-retry.ts` already takes that decision.
- **Patch-vs-refactor verdict:** this is a **refactor**, narrowly scoped. Thirteen call sites each owning a column order is the defect; adding a column to each of them would be the mud-patch. Moving order into the renderer is the smallest change that makes the convention hold by construction rather than by review.

## Discovery (consult log)

- **Consults** — *Owner decisions:* > Indy (2026-09-21): "I request you to order the output on the same order across every command." — asked where a CLI-wide output standard should land and chose "Fold into M204" over its own milestone, a split, and queueing it behind M205; this workstream is that decision, folded into `M204_001` on one branch and one Pull Request. > Indy (2026-09-21): "i think the global commands status must have a fleet_id" — §3. > Indy (2026-09-21): "In secrets i see CREATED, i think you will change it to aGO" — confirmed §1's treatment of `CREATED` columns. > Indy (2026-09-21): "schedule - rename add to create / schedule - rename rm to delete / Schedule - rename status to show / schedule - rename the description in sync to remove Qstash (Re-apply a hosted schedule)" — §6, and the parenthesis is the replacement description verbatim. This supersedes the earlier Out of Scope line, which recorded the verb inconsistency as an open owner decision. > Indy (2026-09-21), of `--wizard`: "Is that needed?" and of `--completions`: "is it tested?" and "Does the --log-level work?" — §2 answers all three; the measurements are below. *Source-verified measurements (Sep 21, 2026, against the built binary and `72c2063f7`):* thirteen `printTable` call sites in `cli/src/commands/` across six column conventions, `api_key.ts:108-112` placing its identifier last against `fleet_library.ts`, `fleet_schedule.ts` and `billing.ts` placing theirs first and `workspace.ts:173-175` opening with a bare `*`; `grep -rc "Effect.log" cli/src` totalling 0, with `--log-level debug` and `--log-level none` producing byte-identical output; `--completions zsh` emitting a `#compdef` script and `bash` 2670 lines, with no behavioural test — its only test reference is the help text at `cli/test/golden/help-no-color.txt:18`; `agentsfleet status [flags]` taking no positional and `status not-a-uuid` refused `EXCESS_ARGUMENTS`; `InstallSourceSelector.tsx:187` reading `entry.visibility` only as a React key. *Architecture:* `docs/architecture/web_app.md` read at authoring on `feat/m204-workspace-library-removal` at `72c2063f7`; it owns the dashboard surfaces the picker label joins, and Dimension 5.1 updates it. *Legacy-Design:* `cli/test/acceptance/lifecycle-with-token.spec.ts:266-269` records "Today only `workspace use`/`delete` run `validateRequiredId`. The fleet / fleet / grant handlers send invalid strings straight to the API — surfaced as Discovery". That claim is measurably false at this revision: `stop`, `kill`, `resume`, `logs` and `api-key delete` each answer `INVALID_ARGUMENT` against an unroutable API URL with no dial. §4 corrects it; the comment's own note that the sweep "widens automatically" once handlers validate is what this Section collects. *Gate-flag triage:* none fired at authoring — no source file was edited.
- **Metrics review** — One operator diagnostic record, `cli.http.attempt`, declared with its privacy guard and test proof. No product analytics event is added and no funnel changes, so no analytics playbook update is required: reshaping a text table and covering two flags moves no funnel step.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/orly-write-integration-test`, `/review` and `orly-babysit-prs` pending, in the order CHORE(close) sets. This workstream is folded into `M204_001`, so its skill-chain runs once for the branch at that spec's close.
- **Deferrals** — none. Nothing is deferred; Out of Scope names what was never in scope, with a reason for each, and the `schedule rm` versus `library remove` spelling is recorded there as an open owner decision rather than as deferred work.
