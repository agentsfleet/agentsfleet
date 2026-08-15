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
**Status:** DONE
**Priority:** P1 — a Command-Line Interface (CLI) user can save a credential naming a provider the runner cannot dial; the save reports success and the failure surfaces later as a fleet that cannot reach any inference host
**Categories:** CLI, DOCS
**Batch:** B1 — §3 is independent of §1/§2 and may land in either order within the same branch
**Branch:** feat/m163-closed-provider-flag
**Test Baseline:** unit=3743 integration=630
**Test Delta (VERIFY):** the Zig depth gate reads `unit=3907 integration=638`, but that **+164 unit / +8 integration is M164's, arriving through the `origin/main` merge** — this diff contains zero Zig (`git diff --name-only origin/main...HEAD | grep '\.zig$'` is empty). This diff's own growth is the Bun `cli` suite: **1471 → 1495, +24** (the closed catalogue and its anchored fixture parity, the typed-form `--provider` requirement, the CLI-engine refusal, `parseEnumOption`'s refusal option and its shadow-drop, the api-key trim, the usage-per-shape pin, the injected-env divergence proof, the unreadable-store warning on both arms, and `cliEnv()`'s own guard).
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5[1m], Aug 12, 2026), verified against source on `main` @ `b941fabf6`; amended and re-verified at PLAN (claude-fable-5, Aug 14, 2026) — every cited path, line, and lane membership re-checked in the worktree
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §9 — Provider routing

---

## Overview

**Goal (testable):** `agentsfleet secret create <name> --provider <unknown>` exits 2 at parse time, names the rejected value and the accepted set, makes no network call, and `runCli({ env })` resolves the credential store from that environment with `process.env.AGENTSFLEET_STATE_DIR` unset.

**Problem:** A user can store a credential naming a provider that has no dial target. Nothing rejects it — the flag takes any non-empty string, the vault stores it, and the command reports success. The failure surfaces later, at the first event, as a fleet that cannot reach an inference host, with nothing pointing back at the typo that caused it. The dashboard has never had this hole: it offers a fixed provider set plus a deliberate custom-endpoint option, so the CLI is the only surface through which an undialable provider enters the system.

**Solution summary:** The accepted provider ids become a named catalogue declared once in the CLI, mirroring the dial names NullClaw's provider factory recognises plus the `openai-compatible` custom-endpoint sentinel. `--provider` on `secret create` and `secret update` parses through that catalogue instead of accepting free text, so rejection happens in commander before any network call, exactly as `--base-url` already rejects a non-https URL. Separately, `resolveStatePaths` stops reading `process.env` directly and takes the environment the caller already resolved — closing the one gap where `runCli`'s `io.env` does not reach, which is why an in-process test must mutate the real process environment to isolate credentials. The config-directory expression that `lib/state.ts` and `services/telemetry/consent.ts` each carried verbatim becomes one shared resolver, so the environment key and the home-directory default have a single declaration site.

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
6. `cli/src/services/telemetry/consent.ts:1-13` — the header records why telemetry reads `process.env` directly (no `CliConfig` environment field yet — the M75 follow-up) and the supabase precedent it mirrors. §3's dedupe gives its `getConfigDir` the shared resolver with an explicit `process.env` argument; it does not thread the environment through the telemetry Effect graph — that is M75's work.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/constants/providers.ts` | CREATE | Single declaration site for the provider ids the CLI accepts |
| `cli/src/program/validators.ts` | EDIT | `parseEnumOption` gains opt-in case folding so no near-duplicate validator is added |
| `cli/src/program/cli-tree-fleet.ts` | EDIT | `--provider` on `secret create` / `secret update` parses through the catalogue; help text names the accepted set |
| `cli/src/lib/config-dir.ts` | CREATE | One declaration site for the config-directory resolution — the environment key and the home default; both prior copies resolve through it |
| `cli/src/lib/state.ts` | EDIT | State paths resolve from a caller-supplied environment instead of reading the process environment |
| `cli/src/services/telemetry/consent.ts` | EDIT | `getConfigDir` resolves through `config-dir.ts` with an explicit `process.env` argument at its single call site; its private copy of the expression is deleted |
| `cli/src/services/credentials.ts` | EDIT | Threads the resolved environment into credential store calls |
| `cli/src/services/workspaces.ts` | EDIT | Threads the resolved environment into workspace store calls |
| `cli/src/cli.ts` | EDIT | Passes the already-resolved environment down to the state layer |
| `cli/test/validators.unit.test.ts` | EDIT | Case-folding and catalogue membership cases |
| `cli/test/cli-tree.fleet.unit.test.ts` | EDIT | Parse-time rejection of an unknown provider on both secret verbs |
| `cli/test/custom-secret-create.integration.test.ts` | EDIT | Typed-form regression under the closed catalogue |
| `cli/test/state.unit.test.ts` | EDIT | Store resolves from caller-supplied environment with the process variable unset |
| `cli/test/config-dir.unit.test.ts` | CREATE | Sibling test: supplied-environment override honoured, home fallback when unset or empty, declaration-site grep |
| `cli/test/json-contract.test.ts` | EDIT | Drops the inline state-directory guard now that the caller's environment reaches the store |
| `cli/test/acceptance/help-and-errors.spec.ts` | EDIT | End-to-end: a subprocess invocation with an unknown provider exits 2 and issues no request. This file is in `DETERMINISTIC_ACCEPTANCE_FILES`, so the case grades without live credentials — correct, because the behaviour under test makes no network call. `secret-vault.spec.ts` is deliberately not used: it sits in `LIVE_ACCEPTANCE_FILES` and would gate a hermetic assertion behind a live deployment |

**Amended at EXECUTE — blast radius the authoring pass under-counted** (each a discovery, none opportunistic):

| File | Action | Why |
|------|--------|-----|
| `cli/src/runtime/main-layer.ts` | EDIT | The hop the table missed: this file wires the two store layers, so it gains an `env` input (default: the process environment, mirroring `runCli`'s own fallback) |
| `cli/src/program/handlers-bind.ts` | EDIT | The second missed hop: composes the layer per invocation and now forwards `ctx.env` — the value `runCli` already resolved |
| `cli/test/helpers-cli-state.ts` | EDIT | Direct store calls gain the env argument; new `stateDirEnv()` call-time helper for injected-env suites; header updated — its "clean fix at that point" note described §3, which now exists |
| ~24 further `cli/test/**` files | EDIT | Mechanical, one shape: direct store calls gain `process.env` as the env argument, and `runCli` env literals gain `...stateDirEnv()` so the injected environment points at the directory the fixture seeded. Repo `tsc --noEmit` covers `test/`, so `make lint-*` enumerates every site — none can be missed silently |
| `cli/test/auth-guard.test.ts` | EDIT | `guardCommand`'s three refusal arms become direct contract tests (errorCode + commanderCode + message). Its DEPLOYMENT_UNKNOWN arm had only incidental coverage through command suites; the env conversion rerouted that path and the 100% line floor caught the gap (`auth-guard.ts:103-106`) |
| `playbooks/lib/runner/runner_test.sh` | EDIT | Indy-acked fold-in at VERIFY: the harness leaked the ambient `AGENTSFLEET_API_URL` into the prod-selection case, so `make check-playbooks` was red on every developer shell (and on `main`); `run_script` now sanitises it — the harness supplies every input a case needs |

**Amended at REVIEW — the second review round's additions** (each traceable to a finding in the Review-findings section):

| File | Action | Why |
|------|--------|-----|
| `cli/src/constants/providers.ts` | EDIT | The catalogue regenerated from `classifyProvider`'s three blocks (116 ids), plus `CLI_ENGINE_PROVIDERS`, its refusal reason, and the help examples |
| `cli/src/commands/fleet_secret_body.ts` | EDIT | The typed form requires `--provider`; `--api-key` trims like its siblings; one usage string per shape, each rejection carrying its own suggestion |
| `cli/src/commands/types.ts` | EDIT | `CommandCtx.env` becomes required — an optional field with a process-environment default was the hop that dropped an injected environment |
| `cli/src/runtime/main-layer.ts` | EDIT | Same, for `MainLayerInput.env`; the header stops describing a `MainLayer` export that no longer exists (RULE NLR) |
| `cli/src/lib/run-effect.ts` | EDIT | The one layer-less test convenience states `process.env` explicitly rather than defaulting inside the layer |
| `cli/src/lib/state-load.ts` | CREATE | The entry point's state read, extracted because the unreadable-file reporting pushed `cli.ts` over the 350-line cap. Records which files failed and their errno; the caller reports once, after it knows which endpoint it settled on |
| `cli/scripts/gen-provider-fixture.ts` | CREATE | Regenerates the provider fixture from vendored source, so a NullClaw bump is mechanical rather than a hand edit |
| `cli/test/fixtures/nullclaw-providers.json` | CREATE | The committed extraction. `zig-pkg/` is gitignored and absent from the Bun-only CI lane, so reading it directly made the parity tests pass locally and fail everywhere else |
| `cli/test/providers-parity.unit.test.ts` | CREATE | Catalogue-vs-fixture and fixture-vs-`build.zig.zon` pin (hermetic, runs everywhere); fixture-vs-live-source drift check where `zig-pkg/` exists |
| `cli/test/injected-env.integration.test.ts` | CREATE | The positive arm of the injected-environment seam, proven by divergence |
| `cli/test/read-path-failures.integration.test.ts` | EDIT | Both arms of the unreadable-store warning — credentials and workspaces — and the endpoint it names |
| `cli/test/cli-linecov.unit.test.ts` | EDIT | Four sites passing a bare env literal resolved the store to the operator's real config directory; routed through `cliEnv()` |
| `cli/README.md` | EDIT | The typed `--provider` rows, the pairing requirement, and a note that an inline `--api-key` is an argv token |

**Spec bookkeeping carried by this branch** (no source change; listed so rubric R5 grades a complete diff):

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M163_001_….md` | MOVE | This spec, `pending/` → `active/` at CHORE(open) |
| `docs/v2/done/M154_002_….md` | MOVE | At Indy's request. PR #598 closed unmerged on Aug 13, 2026, so the park it recorded never reached `main` and the milestone still reads `PENDING` there. The branch copy — `Status: DEFERRED`, the Parked section with Indy's ack quote, the live role-probe table, and the `SET LOCAL role = DEFAULT` finding that supersedes a deferred milestone — lands here verbatim from `origin/feat/m154-privilege-boundaries`, and the stale `pending/` copy is deleted. Spec only: the unmerged grant code stays unmerged, which is what parking means |

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

**Implementation default (amended at REVIEW):** the catalogue mirrors what the vendored NullClaw runtime can dial, which is decided by one function — `classifyProvider` in `zig-pkg/nullclaw-<version>/src/providers/factory.zig`. It reads three blocks, so the catalogue is their union: the `compat_providers` dial table (98), the `core_providers` map (17), and the alias arms of `canonicalProviderName` in `provider_names.zig` (14). Plus `OPENAI_COMPATIBLE_PROVIDER`, agentsfleet's own custom-endpoint sentinel rather than a NullClaw dial target, which the typed form must keep accepting.

The authored default — "the dial names in `docs/architecture/billing_and_provider_keys.md` §9" — was wrong and shipped a regression. That section's table is illustrative, 13 entries; `main` accepts any string, so closing to 13 would have started rejecting `deepseek`, `cerebras`, `mistral` and ~100 more that the runtime dials today. There is no single table to mirror: the doc names one, the factory's own "single source of truth" comment scopes itself to OpenAI-compatible providers only, and core providers plus aliases sit outside it by design.

**CLI-engine carve-out.** `claude-cli`, `claude-code`, `codex-cli`, `gemini-cli`, and `openai-codex` are dialable by NullClaw but excluded. Each spawns a local coding-agent binary (`claude_cli.zig` sets `CLI_NAME = "claude"`) and authenticates through that binary's own session, carrying no API key — so a key-bearing credential naming one is stored, classified `provider_key` by `billing_and_provider_keys.md` §8.2, and can never dial. They are refused by name with that reason rather than drowning in the accepted-set wall. Indy's direction records where they belong instead (see Discovery).

- **Dimension 1.1** — A constants module declares the accepted ids as a single readonly catalogue, importing `OPENAI_COMPATIBLE_PROVIDER` rather than restating it → Test `test_catalogue_has_one_declaration_site` — **DONE**
- **Dimension 1.2** — `parseEnumOption` accepts an opt-in case-folding option; its existing exact-match callers keep exact-match behaviour → Test `test_enum_option_folds_case_only_when_asked` — **DONE**
- **Dimension 1.3** — `--provider` on `secret create` and on `secret update` parses through the catalogue; a canonical id passes and a mixed-case spelling of one normalises to the canonical form → Test `test_provider_flag_accepts_catalogue_and_normalises_case` — **DONE**
- **Dimension 1.4** — The catalogue is re-derived from the vendored source by test, extracting each of the three blocks **by anchor** rather than sweeping the file, so an upstream test-vector row cannot enter a public flag and a dependency bump that widens the dial set fails the suite instead of silently narrowing the flag → Test `test_catalogue_mirrors_classify_provider_by_anchored_extraction` — **DONE**
- **Dimension 1.5** — Every carved-out CLI-engine id is asserted to be a name NullClaw really dials, so the refusal message cannot become a lie if upstream drops one → Test `test_cli_engine_carve_out_names_are_really_dialable` — **DONE**

### §2 — Rejection is immediate, legible, and makes no network call

A rejected provider must cost nothing and explain itself. The user sees the value they typed and the set they may choose from, in the same shape a rejected `--base-url` already produces, so there is one rejection mechanism on this command rather than two.

**Implementation default:** rejection travels commander's `InvalidArgumentError` (exit 2), matching `parseHttpsUrlOption`, because introducing a second rejection path for one flag would leave the two flags on the same command behaving differently.

- **Dimension 2.1** — An unknown provider exits 2 before any request is issued, and the message names both the rejected value and the accepted set → Test `test_unknown_provider_exits_two_without_request` — **DONE**
- **Dimension 2.2** — An empty or whitespace-only `--provider` is rejected with the same code, not treated as absent → Test `test_blank_provider_is_rejected` — **DONE**
- **Dimension 2.3** — `--help` for both secret verbs names example ids and the live accepted count; the full set appears in the unknown-value rejection, which is where an exhaustive list helps rather than buries → Test `test_secret_help_names_examples_and_count` — **DONE** *(amended at REVIEW: the authored dimension put the whole set in help. At 13 ids that read fine; at 116 it is a wall that buries every other option on the command, so the exhaustive list moved to the rejection and help names four examples plus the count.)*
- **Dimension 2.4** — The generic `--data` form still accepts any body, including one whose `provider` field is outside the catalogue → Test `test_data_form_remains_unconstrained` — **DONE**
- **Dimension 2.5** — The typed form requires `--provider`. Commander runs an option parser only on a flag it sees, so `--api-key` with `--model` and no provider reached the composer and produced `provider: ""` — stored by the server as a `provider_key` like any other non-sentinel string, and never dialable. Both verbs refuse it before any request → Test `test_typed_form_without_provider_is_refused` — **DONE**

### §3 — The credential store reads the caller's environment

`runCli` already resolves `io.env ?? process.env` once and places it on the context, but `resolveStatePaths` reads the process environment directly. That is the single hop where an injected environment is dropped, and it is why an in-process test must mutate the real process environment to keep a developer's own credentials out of a case asserting unauthenticated behaviour.

**Implementation default:** the environment is a parameter threaded from the existing context value, not a new module-level accessor, because a second source of environment would reintroduce the divergence this slice removes.

The resolution expression itself is currently written twice — `lib/state.ts:50-54` and `services/telemetry/consent.ts:21-26` are the same three lines — so fixing only the state copy would leave the twin standing (RULE UFS). Both resolve through one shared `resolveConfigDir(env)` in `lib/config-dir.ts`. `consent.ts` passes `process.env` explicitly at its single call site, because its three `getConfigDir` consumers (`login-helpers.ts:200`, `auth-logout.ts:137`, `runtime.layer.ts:92`) have no environment in scope — threading one through the telemetry Effect graph is the M75 follow-up its header already names, and stays out of scope here.

- **Dimension 3.1** — State path resolution takes the environment from its caller; the credential and workspace store functions accept and forward it → Test `test_state_paths_resolve_from_supplied_env` — **DONE**
- **Dimension 3.2** — `runCli({ env })` with a state directory set in that environment and the process variable unset reads and writes under the supplied directory → Test `test_run_cli_env_reaches_credential_store` — **DONE**
- **Dimension 3.3** — `json-contract.test.ts` isolates through the injected environment and no longer mutates the process environment (RULE NLR) → Test `test_json_contract_suite_has_no_process_env_mutation` — **DONE**
- **Dimension 3.4** — The config-directory resolution has one declaration site: neither `state.ts` nor `consent.ts` names the environment key or the home-default tuple after the change → Test `test_config_dir_resolution_has_one_declaration_site` — **DONE**

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

Config-dir resolution (cli/src/lib/config-dir.ts) —
  resolveConfigDir(env: NodeJS.ProcessEnv): string
  STATE_DIR_ENV = "AGENTSFLEET_STATE_DIR"
The one declaration site for the environment key and the ~/.config/agentsfleet
default. state.ts resolves through it with its caller's environment;
telemetry/consent.ts's getConfigDir resolves through it with an explicit
process.env argument (unchanged behaviour for its Effect consumers).
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
3. No function in `cli/src/lib/state.ts` or `cli/src/lib/config-dir.ts` reads the process environment, and the state-directory resolution — the environment key plus the home default — has exactly one declaration site, `config-dir.ts` — enforced by tests that grep both modules for `process.env` (zero matches) and grep `cli/src/` for the environment-key literal outside `config-dir.ts` (zero matches), so a future reintroduction fails the suite rather than review. `consent.ts` keeps `process.env` as an explicit argument at its single `getConfigDir` call site until M75 threads the environment through the telemetry graph.
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
| — | unit | `test_state_module_never_reads_process_env` | Reading `cli/src/lib/state.ts` and `cli/src/lib/config-dir.ts` yields zero `process.env` occurrences (Invariant 3) |
| 3.4 | unit | `test_config_dir_resolution_has_one_declaration_site` | Reading `cli/src/lib/state.ts` and `cli/src/services/telemetry/consent.ts` yields zero occurrences of the environment-key literal and zero of the home-default tuple; `config-dir.ts` declares each exactly once |
| 3.4 | unit | `test_resolve_config_dir_honours_env_and_falls_back` | `resolveConfigDir({ [STATE_DIR_ENV]: "/x" })` → `/x`; `resolveConfigDir({})` and `resolveConfigDir({ [STATE_DIR_ENV]: "" })` → the home default |
| — | unit | `test_absent_state_dir_falls_back_to_home` | A supplied environment with no state directory set yields paths under the home-directory default; no throw, no empty base directory |
| — | integration | `test_unwritable_state_dir_surfaces_write_error` | A read-only supplied directory produces the same write failure and exit code as `main` does today; the environment parameter adds no new failure class |
| — | integration | `test_custom_endpoint_pairing_rules_unchanged` | Regression: the four existing pairing errors (missing key, missing base URL, base URL on a named provider, missing model) produce identical messages and exit codes to `main` |
| — | e2e | `test_cli_rejects_unknown_provider_end_to_end` | Subprocess acceptance in the deterministic lane: the real binary with an unknown provider exits 2, prints the accepted set, and reaches no network — no live deployment required |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An unknown provider is rejected before any network call (§1, §2) | `cd cli && bun run build && bun ./dist/bin/agentsfleet.js secret create t --provider notaprovider --api-key k --model m; echo "exit=$?"` | `exit=2` and stderr contains `must be one of:` | P0 | ✅ `exit=2`; stderr names `notaprovider` + the full accepted set (116 ids after the REVIEW amendment; the pre-amendment run named 13) |
| R2 | The generic `--data` escape hatch is untouched (§2) | `grep -c 'FLAG_PROVIDER.*parseEnumOption' cli/src/program/cli-tree-fleet.ts` | exactly `2` — the two `--provider` sites; `FLAG_DATA_JSON` never pairs with a parser | P1 | ✅ exactly `2` |
| R3 | The state modules never read the process environment (§3, Invariant 3) | `grep -c 'process\.env' cli/src/lib/state.ts cli/src/lib/config-dir.ts` | both lines end `:0` | P0 | ✅ both files `:0` |
| R4 | The public flag change has a documentation branch | `git -C ~/Projects/docs diff --name-only main...HEAD` | at least 1 path, covering the page documenting `secret create --provider` | P0 | ✅ branch `chore/m163-closed-provider-flag-changelog` (pushed): `fleets/model-providers.mdx` (new how-to), `cli/agentsfleet.mdx` (typed-form row), `docs.json` (nav), `changelog.mdx` (`<Update>`). `python3 scripts/check-documentation.py .` → `Documentation check passed` |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 62 paths, 0 missing — the tables were amended twice more (EXECUTE, then REVIEW) as the blast radius grew; the `~24 further cli/test/**` row covers the mechanical `cliEnv()` sweep |
| R6 | The environment-key literal has one src declaration site (§3, Dimension 3.4) | `grep -rn 'AGENTSFLEET_STATE_DIR' cli/src/ \| grep -v 'lib/config-dir.ts'` | no output | P0 | ✅ no output on the final commit; now also suite-enforced by a `Bun.Glob` walk over all of `cli/src/` rather than a hand-run grep |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `EXIT=0` on the final post-review commit, run solo. agentsfleetd 2318 pass/297 skip/**0 fail**; runner 517/7 skip; Zig integration under kcov **906 passed, 7 skipped, 0 failed**; merged Zig line coverage **90.26% ≥ 89%** (28799/31907, 565 files); `enforce-coverage: floor line=100.00% → actual line=100.00% → PASS`; cli suite **1495 pass / 0 fail** |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `All lint checks passed` on the final commit, including the hermetic `env -i` runner harness (17/17 with a hostile `AGENTSFLEET_API_URL` exported) |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ `EXIT=0`, run solo from clean, twice. Corroborated by the coverage lane executing the same binary: **906 passed; 7 skipped; 0 failed**. ⚠️ Open anomaly, reproducible in both runs: the non-TTY log carries a Zig-toolchain `failed command:` line (not ours — zero hits in `make/`, `scripts/`, `build.zig`) on a run that exits 0 and reaches make's success gate, alongside a truncated progress line (`run test agentsfleetd-integration-tests w`). Unexplained; recorded rather than dismissed |
| S4 | End-to-end walks the user path | `make cli-acceptance` | exit 0. Runs the deterministic lane then the live lane; this milestone's new case is in the deterministic half, so a failure there is this diff's and a live-lane failure is not | P0 | ✅ deterministic 95/95 (owns this diff's case) · ⚠️ live lane skipped per environment constraint (AGENTSFLEET_ACCEPTANCE_TARGET unset locally; runs in CI cli-acceptance-{dev,prod}) |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (170.43 MB scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v -E '\.md$\|^vendor/\|_test\.\|\.test\.\|\.spec\.\|/tests?/' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output — *(amended at VERIFY: the authored row omitted the canonical test-file exclusion from `VERIFY_TIERS.md` §Hygiene; spec follows the rule)* | P0 | ✅ no output under the canonical command |
| S9 | Version sync | `make check-version` | exit 0 | P1 | ✅ `all versions match 0.26.2` |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| inline state-directory guard in the JSON suite | `grep -nE 'prevStateDir\|process\.env\.AGENTSFLEET_STATE_DIR' cli/test/json-contract.test.ts` | 0 matches |
| `getConfigDirSync` (consent.ts's private copy of the expression) | `grep -rn 'getConfigDirSync' cli/src/` | 0 matches |
| `node:os` imports orphaned by the shared resolver | `grep -n 'node:os' cli/src/lib/state.ts cli/src/services/telemetry/consent.ts` | 0 matches |

## Out of Scope

- **Reconciling `semconv.WELL_KNOWN_PROVIDERS` with the dial catalogue.** They answer different questions — one is an OpenTelemetry naming standard, the other is what the runner can reach. Folding them together would put private spellings on the wire under a standard key. Separate observability milestone if it is wanted at all.
- **Constraining the `provider` field inside `--data`.** That form is the generic secret blob; §8.2 of the architecture derives `custom_secret` from a missing or non-string provider, so constraining it would break generic secret storage.
- **The CLI-engine providers (`claude-cli`, `claude-code`, `codex-cli`, `gemini-cli`, `openai-codex`).** Dialable by NullClaw, excluded here, and refused by name with the reason. They spawn a local coding-agent binary and authenticate through its own session, so they do not belong on a flag whose whole job is storing an API key. Indy's direction (Discovery, Aug 15) puts them behind an OAuth engine surface — the user clicks to sign in to the tool, and the engine replaces NullClaw for that fleet — which is a different mechanism, a different credential class, and its own milestone.
- **Validating `--model` against the model catalogue.** `--model` is still a free string: any value stores successfully, activates, and fails at the first event — the same store-succeeds/fail-later shape this milestone closes for `--provider`, one flag over. `GET /v1/models` exists (`billing_and_provider_keys.md` §10) and the CLI never calls it; there is no `agentsfleet models` command, so a user choosing a model for their own provider has nothing to check a spelling against. Raised at REVIEW, not folded: it needs a new command, a new endpoint call, and its own docs page.
- **Server-side rejection of an unaccepted provider.** The dashboard and the CLI are the two write surfaces and both would then be constrained, but a server-side closed set changes an API error surface and deserves its own milestone.
- **Adopting m136's hook-scoped state-directory helper across the suite.** `feat/m136-live-connector-proof` adds `useFreshStateDir`; converting files to it belongs to that stream. §3 removes the *cause* of the inline guard rather than restyling it.
- **Threading the caller's environment through the telemetry Effect graph.** `consent.ts:1-2` names it the M75 follow-up — a `CliConfig` environment field its three `getConfigDir` consumers would draw from. §3's dedupe leaves `getConfigDir` passing `process.env` explicitly at one visible call site; converting the consumers is that milestone's work.

---

## Review findings (adversarial pass)

Three CRITICAL findings landed against the implementation and were verified against source before any fix. One was refuted on the facts and kept only as a latent risk.

| # | Finding | Verdict | Landed as |
|---|---|---|---|
| C1 | Omitting `--provider` bypasses the catalogue entirely — `--api-key k --model m` composes `provider: ""`, stored as a `provider_key` that can never dial | **CONFIRMED** from source at `fleet_secret_body.ts:83-113`; none of the four pairing rules rejected the empty string | Dimension 2.5, red-green proven on both verbs |
| C2 | The parity regex sweeps all of `factory.zig`, so 11 `provider_holder_cases` test-vector names enter the public catalogue | **REFUTED as live, CONFIRMED as latent.** Set arithmetic shows every one of the 11 fixture names is also a `core_providers` key — no junk reached the catalogue. The unanchored extraction is still wrong: the next upstream negative fixture (`.name = "unknown-provider"`) would widen a public flag, and the `> 100` assertion was pinned to the accidental 109 rather than the real 98 | Dimension 1.4, anchored per-block extraction at pinned sizes |
| C3 | The catalogue rejects names the runtime dials — `google`, `azure`, `mimo`, `vertex-ai` and 8 more, all working on `main` | **CONFIRMED.** The regex cannot see `core_providers` map keys or `canonicalProviderName` alias arms; only 98 of the union's 120 names are `.name =` fields | §1 implementation default amended; catalogue is the three-block union |

### Second pass (gstack `/review`, Aug 15) — 4 specialists + adversarial, 29 findings

The first pass reviewed the pre-fix state; the catalogue rewrite, the anchored
extraction, the four hardenings and the `cliEnv()` sweep had never been reviewed
by anything but the author. Two findings were ship-blockers.

| # | Finding | Verdict | Landed as |
|---|---|---|---|
| B1 | The parity test read its source of truth from `zig-pkg/`, which is gitignored and produced by no step the Bun-only CI lane runs | **CONFIRMED** — `git ls-files zig-pkg` = 0, `.gitignore:6`; the `test-unit-cli` job is checkout + `bun install` + `bun run test`. Five of seven tests would fail on push, taking the suite and its coverage floor | Committed fixture + a `build.zig.zon` pin assertion, so the guarantee holds with no Zig toolchain; live-source drift check still runs where `zig-pkg/` exists. Proven by removing `zig-pkg/` (9 pass) and by falsifying the pin (fails) |
| B2 | The fixture introduced for B1 was **untracked**, and imported — `Cannot find module`, killing the whole lane | **CONFIRMED**, self-inflicted while fixing B1 | `git add`; plus `cli/scripts/gen-provider-fixture.ts`, which reproduces the committed bytes exactly so a bump is mechanical |
| B3 | `--api-key` was the one typed flag without `.trim()`: a whitespace-only key passed the non-empty gate and stored blank — reported stored, never able to authenticate | **CONFIRMED** | Trimmed like its three siblings and like `resolveApiKeyFromEnv`; red-green proven both ways |
| B4 | One usage string served both shapes of the typed form, so a named-provider error recommended the custom-endpoint line, whose only outcome is the next error | **CONFIRMED** | Split per shape; a rejection carries its own suggestion rather than the renderer printing a usage twice |
| B5 | A test passed a bare `{ NO_COLOR }` env at four sites, resolving the store to the operator's real config directory | **CONFIRMED** — the class `cliEnv()` exists to close, missed because that file never used `stateDirEnv()` | Routed through `cliEnv()` |

**Reversed on review:** the wiring-time overlap check was authored as a throw. These parsers are built at module scope, so a catalogue-data bug would have stopped `--help`, `--version` and `logout` for a user who never touches the flag. It now drops the unfirable refusal; the parity test is what asserts disjointness.

**Not fixed, recorded:** `--api-key` has no stdin form (a feature, annotated in the README instead); `--data` still stores CLI-engine providers the flag refuses (Out of Scope — the generic blob); `custom:` / `anthropic-custom:` prefixes are dialable but rejected, so the mirror claim is narrower than stated; `mkdir` 0o700 covers the parent chain and skips an existing directory; an unparseable state file still warns nothing; telemetry still resolves its directory from `process.env`.

Informational findings also landed: the `main-layer.ts` header documented a `MainLayer` export that does not exist (RULE NLR, fixed in the same diff), and Invariant 3's "suite-enforced" claim was only a three-file check, now a `Bun.Glob` walk over all of `cli/src/`.

Two findings were **rejected**: tightening an already-created config directory's mode (out of this milestone's blast radius — it is a migration concern for existing installs, not a write-path defect), and honouring `env.HOME` in `resolveConfigDir` (the `cliEnv()` guard closes the same hole at lower cost and without changing production path resolution).

## Product Clarity (authoring record)

1. **Successful user moment** — A user types `agentsfleet secret create prod --provider anthropc --api-key … --model …`, and before the shell prompt returns they read `must be one of: anthropic, openai, fireworks, …`, fix the typo, and the credential works the first time it runs.
2. **Preserved user behaviour** — Every existing correct invocation keeps working unchanged: the `--data` form in both spellings including stdin, the typed custom-endpoint form, all four pairing errors, `secret update` replacement semantics, and the home-directory default for the state directory.
3. **Optimal-way check** — The most direct shape would be a catalogue served from the API so a new provider needs no CLI release. That costs a network round-trip on a parse-time check and a fallback for the offline case, to remove a release step that happens rarely. The declared mirror is the accepted gap; `custom-endpoint.ts` already records why this repository mirrors backend literals rather than fetching them.
4. **Rebuild-vs-iterate** — Iterate. The parse layer, the enum factory, and the mirror pattern all exist; this slots into all three. Nothing about it trades away run-to-run determinism.
5. **What we build** — One constants module, one opt-in option on an existing validator, two flag declarations repointed, one environment parameter threaded through the state layer, one shared config-directory resolver replacing the twin copies in `state.ts` and `consent.ts`, and their tests.
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

- **CHORE(open), Aug 14, 2026 — the second production reader of `AGENTSFLEET_STATE_DIR`.**
  §3 as authored scopes the environment threading to `cli/src/lib/state.ts`, and
  Invariant 3 plus rubric R3 grep only that file. Source says there are **two**
  production readers, and they are the same fallback expression written twice:
  `state.ts:50-54` (`resolveStatePaths`) and
  `cli/src/services/telemetry/consent.ts:21-26` (`getConfigDirSync`). Repository-wide
  the variable has 140 occurrences — 2 production reads, 1 documented row in
  `cli/README.md`, and ~137 test sites; nothing under `.github/`, `scripts/`,
  `Makefile`, `make/`, or any Dockerfile sets it, so the test suite is its
  dominant consumer, using a public user knob in place of dependency injection
  (`cli/test/helpers-cli-state.ts:21` records tests clobbering each other's state
  directory as the cost). Fixing `state.ts` alone leaves the duplicate literal
  standing, which is the UFS violation this milestone otherwise closes.
  **Open for Indy at PLAN:** fold `consent.ts` into §3, or record it in Out of
  Scope with a reason. Not decided unilaterally — it widens the blast radius past
  the authored Files Changed table.
- **PLAN, Aug 14, 2026 — resolved: dedupe the expression, do not thread telemetry.**
  > Indy (2026-08-14 19:2x): "Yes plan, but i dont understand this? if its a
  > duplicate use one of them that helps and move on?" — context: the twin
  > config-dir expressions above; decision is the dedupe, not a menu.
  Shape: one shared `resolveConfigDir(env)` in `cli/src/lib/config-dir.ts`; both
  modules resolve through it. `consent.ts` keeps a single explicit `process.env`
  argument at `getConfigDir`, because its three consumers
  (`login-helpers.ts:200`, `auth-logout.ts:137`, `runtime.layer.ts:92`) have no
  environment in scope — threading one through the telemetry Effect graph is the
  M75 follow-up `consent.ts:2` already names, now recorded in Out of Scope.
  Amended in this pass: Files Changed (+3 rows), §3 (+Dimension 3.4), Interfaces,
  Invariant 3, rubric R3 widened + R6 added, Test Specification (+2 rows), Dead
  Code Sweep (+2 rows).
- **EXECUTE §3, Aug 14, 2026 — incident: one old-signature test call clobbered the
  developer's real credential file.** `state.ts` signatures changed (env first),
  and `bun test` transpiles without typechecking, so a not-yet-converted
  `saveCredentials(record)` bound the record as the environment, resolved to the
  home default, and wrote the string `undefined` over
  `~/.config/agentsfleet/credentials.json`. Caught within the same run; the file
  was restored to a valid logged-out record; Indy re-logs-in with
  `agentsfleet login`. `workspaces.json` was untouched (the auth guard failed
  the case before its write). Preventive fact, verified: repo `tsc --noEmit`
  covers `cli/test/**`, and `make lint-cli` runs it — 47 errors enumerated every
  remaining old-signature site, which were then converted in one pass. Lesson:
  after changing an exported signature, run the repo-wide typecheck before
  running any test suite.
- **VERIFY, Aug 14, 2026 — three Indy decisions in one message.**
  > Indy (2026-08-14 ~22:5x): "go for gstack /review · go for yes cross repo
  > update docs · go for yes fold here on check-playbooks one-liner change"
  (1) REVIEW proceeds via gstack `/review`. (2) The `~/Projects/docs` branch for
  the `--provider` page + changelog is authorized this session (rubric R4).
  (3) The `runner_test.sh` ambient-env fix folds into this PR despite sitting
  outside the authored Files Changed — recorded as its own table row.
- **REVIEW, Aug 15, 2026 — resolved: the catalogue mirrors `classifyProvider`, and the CLI engines wait for an OAuth engine surface.**
  Context: the authored catalogue premise ("mirror the factory") does not resolve —
  `classifyProvider` reads three separate blocks, and the doc table it was authored
  from is 13 illustrative entries. Presented with the set arithmetic (98 compat + 17
  core + 14 aliases = 120 dialable, against a 108-entry catalogue missing 12 names
  `main` accepts today).
  > Indy (2026-08-15): "well i just need 1 source of truth meaning what nullclaw
  > providers are in our factory.zig or facotry.zig is nullclass."
  Resolution: one source of truth is the **function**, not a table — the catalogue
  mirrors `classifyProvider`'s accept set, and the parity test extracts the three
  blocks it consults, by anchor.
  On the CLI engines, asked whether NullClaw carries `claude-cli` / `codex-cli` /
  `ampcode` as providers (answer from source: the first two yes, as first-class
  `ProviderKind` variants that spawn a local binary and take no API key; `ampcode`
  does not exist in NullClaw at all):
  > Indy (2026-08-15): "suggest me this area for later implementation on how i feel
  > can support all the cli, they would get added in a different way where the user
  > clicks login to codex-cli which opens the codex login and upon auth we start
  > using codex-cli as opposed to nullclaw so its pretty much an engine abstraction?
  > via oauth"
  Resolution: the CLI engines are **deferred to a future engine surface** and
  excluded from this key-bearing flag, refused by name with the reason. Recorded in
  Out of Scope; the parity test asserts each carved-out id is genuinely dialable so
  the refusal cannot become a lie.
  Amended in this pass: §1 implementation default, §1 (+Dimensions 1.4, 1.5), §2
  (+Dimension 2.5, 2.3 rewritten), Review findings table, rubric R1 + R4 regraded,
  Out of Scope (+2 rows).

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
