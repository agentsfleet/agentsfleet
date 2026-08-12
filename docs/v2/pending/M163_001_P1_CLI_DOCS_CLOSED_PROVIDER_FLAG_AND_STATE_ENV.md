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

# M163_001: `--provider` accepts only names the runtime can dial, and the credential store reads the caller's environment

**Prototype:** v2.0.0
**Milestone:** M163
**Workstream:** 001
**Date:** Aug 12, 2026
**Status:** PENDING
**Priority:** P1 — a Command-Line Interface (CLI) user can save a credential naming a provider the runner cannot dial; the save reports success and the failure surfaces later as a fleet that cannot reach any inference host
**Categories:** CLI, DOCS
**Batch:** B1 — §3 is independent of §1/§2 and may land in either order within the same branch
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5[1m], Aug 12, 2026), verified against source on `main` @ `b941fabf6`
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §9 — Provider routing

---

## Overview

**Goal (testable):** `agentsfleet secret create <name> --provider <unknown>` exits 2 at parse time, names the rejected value and the accepted set, makes no network call, and `runCli({ env })` resolves the credential store from that environment with `process.env.AGENTSFLEET_STATE_DIR` unset.

**Problem:** A user can store a credential naming a provider that has no dial target. Nothing rejects it — the flag takes any non-empty string, the vault stores it, and the command reports success. The failure surfaces later, at the first event, as a fleet that cannot reach an inference host, with nothing pointing back at the typo that caused it. The dashboard has never had this hole: it offers a fixed provider set plus a deliberate custom-endpoint option, so the CLI is the only surface through which an undialable provider enters the system.

**Solution summary:** The accepted provider ids become a named catalogue declared once in the CLI, mirroring the dial names NullClaw's provider factory recognises plus the `openai-compatible` custom-endpoint sentinel. `--provider` on `secret create` and `secret update` parses through that catalogue instead of accepting free text, so rejection happens in commander before any network call, exactly as `--base-url` already rejects a non-https URL. Separately, `resolveStatePaths` stops reading `process.env` directly and takes the environment the caller already resolved — closing the one gap where `runCli`'s `io.env` does not reach, which is why an in-process test must mutate the real process environment to isolate credentials.

**Cross-repository note:** `--provider` is a public flag, so its behaviour change requires a matching branch in `~/Projects/docs`. That repository is never edited through this worktree.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m163): close the --provider flag to dialable providers`
- **Intent (one sentence):** A mistyped or unsupported provider fails at the moment the user types it, with the accepted set on screen, instead of succeeding into a credential that cannot run.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/src/constants/custom-endpoint.ts` — the established mirror pattern. It states why the `cli/` and `ui/` Bun projects each re-declare a backend literal exactly once, and it already owns `OPENAI_COMPATIBLE_PROVIDER`. The new catalogue follows this file's shape, and must not duplicate that constant.
2. `cli/src/program/validators.ts` — `parseEnumOption` already does closed-set membership with an `InvalidArgumentError`; `parseHttpsUrlOption` is the precedent for rejecting at parse time with no network call. Read the module header before adding anything: it explains the direct-vs-factory split.
3. `docs/architecture/billing_and_provider_keys.md` §9 — the dial catalogue table (provider name → endpoint → wire format) and §8.2, which derives a stored credential's `kind` from its `provider` field.
4. `cli/src/cli.ts` around `runCli` — `io.env ?? process.env` is already resolved once and placed on the context; §3 threads that value the last hop rather than introducing a new source of environment.
5. `schema/400_model_library.sql` header — records that provider values are app-enforced named constants and never a SQL `CHECK` (RULE STS). This milestone supplies the app enforcement that comment assumes.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/constants/providers.ts` | CREATE | Single declaration site for the provider ids the CLI accepts |
| `cli/src/program/validators.ts` | EDIT | `parseEnumOption` gains opt-in case folding so no near-duplicate validator is added |
| `cli/src/program/cli-tree-fleet.ts` | EDIT | `--provider` on `secret create` / `secret update` parses through the catalogue; help text names the accepted set |
| `cli/src/lib/state.ts` | EDIT | State paths resolve from a caller-supplied environment instead of reading the process environment |
| `cli/src/services/credentials.ts` | EDIT | Threads the resolved environment into credential store calls |
| `cli/src/services/workspaces.ts` | EDIT | Threads the resolved environment into workspace store calls |
| `cli/src/cli.ts` | EDIT | Passes the already-resolved environment down to the state layer |
| `cli/test/validators.unit.test.ts` | EDIT | Case-folding and catalogue membership cases |
| `cli/test/cli-tree.fleet.unit.test.ts` | EDIT | Parse-time rejection of an unknown provider on both secret verbs |
| `cli/test/custom-secret-create.integration.test.ts` | EDIT | Typed-form regression under the closed catalogue |
| `cli/test/state.unit.test.ts` | EDIT | Store resolves from caller-supplied environment with the process variable unset |
| `cli/test/json-contract.test.ts` | EDIT | Drops the inline state-directory guard now that the caller's environment reaches the store |
| `cli/test/acceptance/secret-vault.spec.ts` | EDIT | End-to-end: an unknown provider exits 2 against a live target with no vault write |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NTP** (the driver: parse external input into a finite value set at the boundary, never store the raw string), **CFG** (the catalogue is a data table, so adding a provider is one array entry and never a new function or switch arm), **UFS** (every provider id is a named constant with one declaration site), **TFX** (tests import the production catalogue instead of restating ids), **JCL** (stable Command-Line Interface error codes; usage text is never the error message), **NLR** (touch-it-fix-it: the inline state-directory guard in `json-contract.test.ts` goes when its cause is removed), **NDC** (no dead code at write time), **FLL** (file and function length caps).
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — every file in this diff is TypeScript. TS FILE SHAPE DECISION at PLAN, `const` and import discipline, Bun-primitive preference.
- `~/Projects/dotfiles/docs/EXECUTE_DOC_READS.md` — the trigger map for this repository; DOC READ GATE proof lines are emitted per triggered document per turn.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` file is touched | N/A |
| PUB / Struct-Shape | no — no Zig public surface | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — `validators.ts` is 242 lines and `cli-tree-fleet.ts` 194; both grow | Catalogue lives in its own constants module rather than inside either file; if `validators.ts` approaches the cap, the enum factory moves with its siblings, not the callers |
| UFS (repeated/semantic literals) | yes — every provider id is a new literal | All ids declared once in `cli/src/constants/providers.ts`; `OPENAI_COMPATIBLE_PROVIDER` is imported from `custom-endpoint.ts`, never restated; tests import the catalogue |
| UI Substitution / DESIGN TOKEN | no — no `ui/` file is touched | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — rejection travels commander's existing `InvalidArgumentError` path; no new `UZ-*` code, no schema change | N/A |

## Prior-Art / Reference Implementations

- **Reference:** `cli/src/program/cli-tree-access.ts:47` — `--sort` already parses through `parseEnumOption(API_KEY_SORTS)` against a catalogue declared elsewhere. `--provider` becomes the second instance of a shape this repository has already accepted, not a new mechanism.
- **Reference:** `cli/src/constants/custom-endpoint.ts` — the one-declaration-site mirror of a backend literal, with the reason recorded in the file header.
- **CLI developer-experience pillars:** aligns with *command → handler → errors split* (validation stays in the parse layer, handlers stay pure), *structured errors with a suggestion field* (the accepted set is the suggestion), and the *three-tier test pyramid* (parse unit, in-process integration, subprocess acceptance). No divergence.

## Sections (implementation slices)

### §1 — The accepted provider set becomes a named catalogue

Today `--provider` takes any non-empty string, so the CLI is the entry point for provider values the runner cannot dial. This slice gives the CLI the catalogue the architecture already assumes exists, declared once so a future provider is one entry.

**Implementation default:** the catalogue holds the dial names in `docs/architecture/billing_and_provider_keys.md` §9 plus `OPENAI_COMPATIBLE_PROVIDER`, because `openai-compatible` is agentsfleet's own custom-endpoint sentinel rather than a NullClaw dial target, and the typed form must keep accepting it.

- **Dimension 1.1** — A constants module declares the accepted ids as a single readonly catalogue, importing `OPENAI_COMPATIBLE_PROVIDER` rather than restating it → Test `test_catalogue_has_one_declaration_site`
- **Dimension 1.2** — `parseEnumOption` accepts an opt-in case-folding option; its existing exact-match callers keep exact-match behaviour → Test `test_enum_option_folds_case_only_when_asked`
- **Dimension 1.3** — `--provider` on `secret create` and on `secret update` parses through the catalogue; a canonical id passes and a mixed-case spelling of one normalises to the canonical form → Test `test_provider_flag_accepts_catalogue_and_normalises_case`

### §2 — Rejection is immediate, legible, and makes no network call

A rejected provider must cost nothing and explain itself. The user sees the value they typed and the set they may choose from, in the same shape a rejected `--base-url` already produces, so there is one rejection mechanism on this command rather than two.

**Implementation default:** rejection travels commander's `InvalidArgumentError` (exit 2), matching `parseHttpsUrlOption`, because introducing a second rejection path for one flag would leave the two flags on the same command behaving differently.

- **Dimension 2.1** — An unknown provider exits 2 before any request is issued, and the message names both the rejected value and the accepted set → Test `test_unknown_provider_exits_two_without_request`
- **Dimension 2.2** — An empty or whitespace-only `--provider` is rejected with the same code, not treated as absent → Test `test_blank_provider_is_rejected`
- **Dimension 2.3** — `--help` for both secret verbs lists the accepted set, so the valid values are discoverable without reading documentation → Test `test_secret_help_lists_accepted_providers`
- **Dimension 2.4** — The generic `--data` form still accepts any body, including one whose `provider` field is outside the catalogue → Test `test_data_form_remains_unconstrained`

### §3 — The credential store reads the caller's environment

`runCli` already resolves `io.env ?? process.env` once and places it on the context, but `resolveStatePaths` reads the process environment directly. That is the single hop where an injected environment is dropped, and it is why an in-process test must mutate the real process environment to keep a developer's own credentials out of a case asserting unauthenticated behaviour.

**Implementation default:** the environment is a parameter threaded from the existing context value, not a new module-level accessor, because a second source of environment would reintroduce the divergence this slice removes.

- **Dimension 3.1** — State path resolution takes the environment from its caller; the credential and workspace store functions accept and forward it → Test `test_state_paths_resolve_from_supplied_env`
- **Dimension 3.2** — `runCli({ env })` with a state directory set in that environment and the process variable unset reads and writes under the supplied directory → Test `test_run_cli_env_reaches_credential_store`
- **Dimension 3.3** — `json-contract.test.ts` isolates through the injected environment and no longer mutates the process environment (RULE NLR) → Test `test_json_contract_suite_has_no_process_env_mutation`

## Interfaces

```
agentsfleet secret create <name> --provider <id> [--base-url <url>] [--api-key <key>] --model <name>
agentsfleet secret update <name> --provider <id> [--base-url <url>] [--api-key <key>] --model <name>

  <id> ∈ the declared catalogue (case-folded to canonical on accept).
  Rejection: exit 2, stderr `error: option '--provider <id>' argument '<value>'
  is invalid. must be one of: <accepted set>` — commander's existing rendering,
  unchanged in shape from a rejected --base-url.

  --data <json> is unchanged and remains unconstrained.

State layer (cli/src/lib/state.ts) — every exported store function takes the
caller's environment; no function in the module reads the process environment.
The exported shapes (Credentials, Workspaces, WorkspaceItem, StatePaths) and
the on-disk file names, locations, and 0o600 mode are unchanged.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown provider | Typo, or a provider with no dial target | Rejected in commander; exit 2; no network call, no vault write; message names the value and the accepted set |
| Blank provider | `--provider ""` or whitespace | Rejected with the same code; never silently treated as an absent flag |
| Case variant | `Anthropic` for `anthropic` | Accepted and normalised to the canonical id, so the stored body always carries the canonical spelling |
| Custom endpoint without a base URL | `--provider openai-compatible` with no `--base-url` | Existing pairing error is unchanged; catalogue membership does not bypass it |
| Named provider with a base URL | A catalogue provider plus `--base-url` | Existing rejection is unchanged; `--base-url` stays exclusive to the custom-endpoint sentinel |
| Environment missing a state directory | `runCli({ env })` with no state directory set | Falls back to the home-directory default exactly as today; no throw, no empty base directory |
| Unwritable state directory | Supplied directory is read-only | The existing write error surfaces unchanged; the environment threading adds no new failure class |

## Invariants

1. Every provider id has exactly one declaration site — enforced by the UFS end-of-turn literal audit over the diff, and by tests importing the catalogue rather than restating ids (RULE TFX).
2. `--provider` cannot carry a value outside the catalogue past the parse layer — enforced by the commander parser, which throws before any handler runs; the handler never re-checks and cannot be reached with an unaccepted value.
3. No function in `cli/src/lib/state.ts` reads the process environment — enforced by a test that greps the module for `process.env` and asserts zero matches, so a future reintroduction fails the suite rather than review.
4. Adding a provider requires no new function and no new switch arm — enforced by RULE CFG's shape: the catalogue is data, and the parser is generic over it.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | Rejection happens client-side before any request, so no server-side counter observes it, and no CLI telemetry event is added | not applicable | no credential material reaches any output; the rejected value is a provider id, never a key | `test_unknown_provider_exits_two_without_request` asserts zero requests issued |

`agentsfleet_otel_attribute_omitted_total{provider_name, unmapped_provider}` is **deliberately unchanged**. That counter compares a stored provider against `semconv.WELL_KNOWN_PROVIDERS`, the OpenTelemetry semantic-convention list — not against dialability. Supported providers including `fireworks` (the platform default), `openrouter`, `moonshot`, `kimi`, and `together` are absent from that list by design, so the counter continues to fire on supported paths after this milestone, and that remains correct: exporting a non-standard spelling under a standard attribute key would claim interoperability the value does not have. Metrics review: no analytics or funnel playbook update required, because no user-facing funnel step changes.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_catalogue_has_one_declaration_site` | The catalogue contains the custom-endpoint sentinel, and the constants module declares that literal zero times of its own |
| 1.2 | unit | `test_enum_option_folds_case_only_when_asked` | Default factory rejects `"DEV"` against `["dev","prod"]`; the case-folding variant accepts it and returns `"dev"` |
| 1.3 | unit | `test_provider_flag_accepts_catalogue_and_normalises_case` | `--provider anthropic` and `--provider Anthropic` both parse; both yield `anthropic` on both secret verbs |
| 2.1 | integration | `test_unknown_provider_exits_two_without_request` | `secret create x --provider notaprovider --api-key k --model m` → exit 2, stderr names `notaprovider` and the accepted set, zero requests observed by the stub transport |
| 2.2 | unit | `test_blank_provider_is_rejected` | `--provider ""` and `--provider "  "` → exit 2, not routed to the missing-`--data` hint |
| 2.3 | unit | `test_secret_help_lists_accepted_providers` | `secret create --help` and `secret update --help` output each contain every catalogue id |
| 2.4 | integration | `test_data_form_remains_unconstrained` | `secret create x --data '{"provider":"notaprovider","model":"m"}'` → exit 0, body posted verbatim |
| 3.1 | unit | `test_state_paths_resolve_from_supplied_env` | A supplied environment naming a temporary directory yields credential and workspace paths under it while the process variable is unset |
| 3.2 | integration | `test_run_cli_env_reaches_credential_store` | `runCli({ env })` with credentials seeded in the supplied directory authenticates; with an empty supplied directory the same command reports unauthenticated, process environment untouched throughout |
| 3.3 | unit | `test_json_contract_suite_has_no_process_env_mutation` | Reading `cli/test/json-contract.test.ts` yields zero assignments to `process.env` |
| — | unit | `test_state_module_never_reads_process_env` | Reading `cli/src/lib/state.ts` yields zero `process.env` occurrences (Invariant 3) |
| — | unit | `test_absent_state_dir_falls_back_to_home` | A supplied environment with no state directory set yields paths under the home-directory default; no throw, no empty base directory |
| — | integration | `test_unwritable_state_dir_surfaces_write_error` | A read-only supplied directory produces the same write failure and exit code as `main` does today; the environment parameter adds no new failure class |
| — | integration | `test_custom_endpoint_pairing_rules_unchanged` | Regression: the four existing pairing errors (missing key, missing base URL, base URL on a named provider, missing model) produce identical messages and exit codes to `main` |
| — | e2e | `test_secret_vault_rejects_unknown_provider` | Subprocess acceptance against a live target: unknown provider exits 2 and the vault list is unchanged afterwards |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An unknown provider is rejected before any network call (§1, §2) | `cd cli && bun run build && ./dist/agentsfleet secret create t --provider notaprovider --api-key k --model m; echo "exit=$?"` | `exit=2` and stderr contains `must be one of:` | P0 | |
| R2 | The generic `--data` escape hatch is untouched (§2) | `grep -c 'FLAG_PROVIDER.*parseEnumOption' cli/src/program/cli-tree-fleet.ts` | exactly `2` — the two `--provider` sites; `FLAG_DATA_JSON` never pairs with a parser | P1 | |
| R3 | The state module never reads the process environment (§3, Invariant 3) | `grep -c 'process\.env' cli/src/lib/state.ts` | `0` | P0 | |
| R4 | The public flag change has a documentation branch | `git -C ~/Projects/docs diff --name-only main...HEAD` | at least 1 path, covering the page documenting `secret create --provider` | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | End-to-end walks the user path | `make cli-acceptance` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Version sync | `make check-version` | exit 0 | P1 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| inline state-directory guard in the JSON suite | `grep -n 'prevStateDir' cli/test/json-contract.test.ts` | 0 matches |
| now-unused imports left by that removal | `grep -nE 'mkdtempSync\|node:os\|node:path' cli/test/json-contract.test.ts` | 0 matches |

## Out of Scope

- **Reconciling `semconv.WELL_KNOWN_PROVIDERS` with the dial catalogue.** They answer different questions — one is an OpenTelemetry naming standard, the other is what the runner can reach. Folding them together would put private spellings on the wire under a standard key. Separate observability milestone if it is wanted at all.
- **Constraining the `provider` field inside `--data`.** That form is the generic secret blob; §8.2 of the architecture derives `custom_secret` from a missing or non-string provider, so constraining it would break generic secret storage.
- **Server-side rejection of an unaccepted provider.** The dashboard and the CLI are the two write surfaces and both would then be constrained, but a server-side closed set changes an API error surface and deserves its own milestone.
- **Adopting m136's hook-scoped state-directory helper across the suite.** `feat/m136-live-connector-proof` adds `useFreshStateDir`; converting files to it belongs to that stream. §3 removes the *cause* of the inline guard rather than restyling it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user types `agentsfleet secret create prod --provider anthropc --api-key … --model …`, and before the shell prompt returns they read `must be one of: anthropic, openai, fireworks, …`, fix the typo, and the credential works the first time it runs.
2. **Preserved user behaviour** — Every existing correct invocation keeps working unchanged: the `--data` form in both spellings including stdin, the typed custom-endpoint form, all four pairing errors, `secret update` replacement semantics, and the home-directory default for the state directory.
3. **Optimal-way check** — The most direct shape would be a catalogue served from the API so a new provider needs no CLI release. That costs a network round-trip on a parse-time check and a fallback for the offline case, to remove a release step that happens rarely. The declared mirror is the accepted gap; `custom-endpoint.ts` already records why this repository mirrors backend literals rather than fetching them.
4. **Rebuild-vs-iterate** — Iterate. The parse layer, the enum factory, and the mirror pattern all exist; this slots into all three. Nothing about it trades away run-to-run determinism.
5. **What we build** — One constants module, one opt-in option on an existing validator, two flag declarations repointed, one environment parameter threaded through the state layer, and their tests.
6. **What we do NOT build** — No served catalogue, no server-side provider rejection, no change to the `--data` form, no OpenTelemetry reconciliation, no did-you-mean suggestion beyond listing the accepted set.
7. **Fit with existing features** — Compounds with the typed custom-endpoint form and with `tenant provider create`, which resolves against the model library. It must not destabilise the custom-endpoint path: `openai-compatible` is a catalogue member, and the existing pairing rules run unchanged after membership passes.
8. **Surface order** — CLI-only. The dashboard already constrains provider selection, so this closes the gap rather than opening a new surface.
9. **Dashboard restraint** — No dashboard change. Nothing here produces a signal worth a control or a counter.
10. **Confused-user next step** — The rejection message itself: it names the accepted set, which is the whole answer. `agentsfleet secret create --help` repeats it. Neither path requires a support request.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, three sections. §1 and §2 are one behaviour split by concern — the set, then the rejection — so each has its own tests and can be marked DONE independently. §3 is separable but belongs in the same PR: it is the same file family, and leaving it out means the JSON suite keeps mutating the process environment for a reason this milestone otherwise removes.
- **Alternatives considered:** (a) *Validate server-side only* — catches the dashboard too, but costs a round-trip to learn a typo and changes a public API error surface; rejected as a larger, separately-reviewable change. (b) *Warn without rejecting* — preserves every workflow, but a warning on a command that then reports success is the failure mode being fixed, not a fix. (c) *Fetch the catalogue from the API at parse time* — see Product Clarity item 3.
- **Patch-vs-refactor verdict:** this is a **patch** because every mechanism it needs already exists and is already used elsewhere in the same layer; the change is which parser a flag declaration names, plus one parameter threaded through five call sites. The refactor that would genuinely improve on it — a served provider catalogue shared by the CLI, the dashboard, and the resolver — is named in Out of Scope rather than half-built here.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
