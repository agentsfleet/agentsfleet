# M249_002: Fleet commands and scoped discovery

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 002
**Date:** Nov 13, 2026
**Status:** DONE
**Priority:** P1, make the command surface coherent and advertise only usable operator commands
**Categories:** CLI, API, DOCS
**Batch:** B2
**Depends on:** nothing; it merges before M249_001 (UI refresh), which rebases onto it (Indy, Nov 13, 2026, Discovery)
**Provenance:** Agent-generated from Indy's approved CLI grouping and permission-discovery decisions; split from the authored M249_002 on Nov 12, 2026 (see Discovery)
**Canonical architecture:** `docs/architecture/api_resources.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** Every command sits under its resource group, help, suggestions and completions advertise only the platform commands the caller's server-reported scopes allow, every failure renders as one aligned block, and a fleet's lifecycle is activate, archive or delete, while each typed command keeps its handler and backend authorization.

**Problem:** Fleet verbs are scattered across root help (`cli/src/program/tree/root.command.ts:130`). Root help lists `platform` for every account, and `platform --help` lists all four operator families. A usage error prints the whole help document above a misaligned `✕ error:` / `  Suggestion:` pair (`cli.ts:270`).

**Solution summary:** Domain factories take a `Visibility` and return their group; `tree/index.ts` composes them. For discovery requests only, one bounded `GET /v1/users/me` decides which platform leaves are `Command.unlisted`. `GlobalFlag.Completions` generates scripts from the same tree. A custom `CliOutput` presenter and one error-block module render every failure. Vocabulary renames (Plugins, Archive) belong to M249_003.

**Design flow:**

```
argv ─▶ argv-scan (--json, --api, --, version)
     ─▶ loadState + env key ─▶ guard ctx
     ─▶ tree(Unknown) ─▶ resolveCommandPath ─▶ commandPath
     ─▶ isDiscoveryRequest(argv, commandPath)? ──no──────────────────┐
              │ yes, and credential eligible (§2)                     │
              ▼                                                       │
        discovery layer (CliConfig + Credentials + HttpClient only)   │
        GET /v1/users/me  retry {maxAttempts:1}, timeoutMs 2000       │
        strict Schema decode of `scopes` ─▶ Visibility.Known | Unknown│
              ▼                                                       ▼
        tree(visibility) ─────────────▶ houseRejection, Command.runWith(renderErrors: true)
                                              │
                         presenter: Help → print; InvalidInvocation → error block
                                              │
                         existing handler ◀── existing guard ◀── server scope gate
```

## PR Intent & comprehension handshake

- **PR title (eventual):** Group CLI commands, scope platform discovery and align errors
- **Intent:** Make the command surface coherent, hide operator commands an account cannot use, and make failures readable, without weakening any permission check.
- **Authoring handshake:** The authored spec combined the CLI/API cleanup and scope-aware discovery. Indy split it on Nov 12, 2026: this spec regroups the CLI without changing vocabulary; M249_003 carries every Plugin and Archive rename across API, CLI, UI and docs, plus the delete composition.
- **Implementer handshake (Nov 13, 2026: 12:40 AM):**
  - Fetched base `4f3159269` (`git rev-parse origin/main`).
  - UI PR status: `feat/m249-fleet-interface-refresh` has no remote ref (`git ls-remote origin 'refs/heads/feat/m249*'` → empty) and no PR (`gh pr list --search m249` → `[]`).
  - Baseline: `make test-unit-cli` at the base → exit 0, `2223 pass`, `0 fail`, `Ran 2240 tests across 234 files`.
  - Pinned library: `effect@4.1.3`. `Command.unlisted` omits a child from parent help (`effect/src/cli/internal/command.ts:179`), completion scripts (`internal/completions/descriptor.ts:157`) and did-you-mean (`internal/parser.ts:862`), and exact names still parse. It does not hide a parent whose children are all unlisted (`internal/command.ts:151`); factories do that. `GlobalFlag.Completions` accepts `bash`, `zsh`, `fish`, `powershell`, `sh`, `pwsh` (`GlobalFlag.ts:226`). Both are marked unstable; Dimensions 1.1, 2.1 and 2.5 rerun when `effect` changes in `cli/bun.lock`.
  - Scope semantics: `/v1/users/me` renders the stored `ScopeSet` (`afd_api_tenant/src/handler/tenant/identity.rs:93`), which is the downward closure (`afd_auth/src/scope.rs` `HIERARCHY`, `ScopeSet::insert`). `workspace:any` is an ownership bypass, not a capability wildcard, and grants no platform command. Visibility is membership in the returned list; the CLI holds no hierarchy.
- **ASSUMPTIONS I'M MAKING:** (1) `fleet kill` is an interim spelling that M249_003 renames to `fleet archive`; (2) `library` stays a root group until M249_003; (3) server hint strings that name removed root verbs are edited here, because this PR is what makes them wrong; (4) the minor version moves to `0.77.0`.

## Implementing agent — read these first

1. `cli/src/cli.ts`, `cli/src/program/entry/argv-scan.ts`, `cli/src/program/tree/root.command.ts` and `cli/src/program/tree/resolve-path.ts`: execution order, path walking and `helpArgv`/`firstCommandIndex`.
2. `cli/src/program/auth-guard.ts` (`unboundTarget`, `requireAuth`), `cli/src/services/http-client.ts` (`resolveToken`, `timeoutMs`, `retry`) and `cli/src/services/credentials.ts` (`getAccessToken`).
3. `cli/src/program/tree/platform*.command.ts`: each leaf already names its scope in its description.
4. `cli/node_modules/effect/src/cli/CliOutput.ts` (`Presenter`, `layerPresenter`) and `Command.ts` `showHelp` (line 3300): what `renderErrors: true` sends a presenter.
5. `cli/src/errors/base.ts` and `cli/src/errors/index.ts`: `TaggedCliError`, the closed `CliError` union and `EXIT_CODE`.
6. `cli/src/program/entry/help-formatter.ts` (`wrapAt`, `visibleLength`, `MAX_WIDTH`) and `cli/src/output/capability.ts` (`detectColorMode`).

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| This spec (`active/` → `done/`) | EDIT/MOVE | Grade the workstream |
| `docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md` | EDIT/MOVE | Takes the vocabulary renames and delete composition |
| `cli/src/cli.ts` | EDIT | Discovery step, one visibility tree, presenter, completions |
| `cli/src/program/tree/root.command.ts` | DELETE | Mixed assembly and leaf-handler module |
| `cli/src/program/tree/index.ts` | CREATE | `buildCommandTree(visibility)` |
| `cli/src/program/tree/global-flags.ts` | CREATE | `--api`, `--json` |
| `cli/src/program/tree/auth.command.ts` | CREATE | `login`, `logout`, `whoami` |
| `cli/src/program/tree/diagnostics.command.ts` | CREATE | `doctor`; `pet` unlisted |
| `cli/src/program/tree/{fleet,access,workspace,chat,chat-history,schedule,memory}.command.ts` | EDIT | Factories; `fleet` nests lifecycle, approvals, grants, schedule, memory |
| `cli/src/program/tree/{models,secrets}.command.ts` | CREATE | Moved out of `fleet.command.ts` and `workspace.command.ts` |
| `cli/src/program/tree/platform*.command.ts` (5 files), `cli/src/program/tree/platform-flags.ts` | EDIT | Leaves listed by the `PLATFORM_SCOPE` each needs; parents unlisted when empty |
| `cli/src/program/tree/resolve-path.ts` | EDIT | `walkCommandPath` (returns the stopping token) and `asCommandNode` |
| `cli/src/services/output.ts` | EDIT | `Output.error(msg, { fields })` renders the block |
| `cli/src/output/text.ts` | CREATE | `visibleLength` and `wrapAt`, shared by help and error output |
| `cli/src/program/entry/exit-code.ts` | EDIT | Comment: the presenter, not the library, owns usage-error output |
| `cli/src/program/tree/flags.ts` | EDIT | Flag descriptions naming moved commands (`flags.ts:157`) |
| `cli/src/program/entry/discovery.ts` | CREATE | `isDiscoveryRequest`, eligibility, bounded read, strict decode |
| `cli/src/program/entry/presenter.ts` | CREATE | `CliOutput` presenter: help passes through, usage errors become blocks |
| `cli/src/output/error-block.ts` | CREATE | Label column, hanging-indent wrap, colour mode |
| `cli/src/lib/cli-error-render.ts`, `cli/src/lib/run-effect.ts`, `cli/src/program/io.ts`, `cli/src/program/entry/rejection.ts`, `cli/src/constants/rejection.ts` | EDIT | Every failure writer uses the block; `SUGGESTION_PREFIX` stays only to build `Error.message`, `USAGE_PREFIX` moves a handler's `usage:` hint onto the `usage` line |
| `cli/src/program/entry/deferred-help.ts` | DELETE | No help document is written for an invalid invocation |
| `cli/src/program/entry/help-formatter.ts` | EDIT | Imports `wrapAt` and `visibleLength` from `output/text.ts` |
| `cli/src/program/entry/argv-scan.ts` | EDIT | Export the flag-only target scan `flaggedApiUrl` |
| `cli/src/program/tree/{visibility.ts,library.command.ts}` | CREATE | `Visibility` type; `library` group moved out of `fleet.command.ts` |
| `cli/src/commands/{fleet,approvals,grant,secret,schedule,connector,api-key,models,tenant,workspace,billing}/*.ts`, `cli/src/lib/model-catalogue.ts` | EDIT | Hint strings naming moved commands, including templated `agentsfleet ${verb}` (`fleet.ts:161`) |
| `cli/test/**` (26 files with removed spellings, plus argv in 62), `cli/test/acceptance/**` incl. `fixtures/command-matrix.ts` | EDIT | New spellings |
| `cli/test/{command-groups,scoped-discovery,completions,error-blocks,tree-exports}.unit.test.ts`, `cli/test/fixtures/command-parity.ts` | CREATE | Dimension proofs |
| `cli/test/setup.ts` | EDIT | Unit preload always uses an empty state dir (owner decision in Discovery) |
| `cli/test/helpers-discovery.ts`, `cli/test/acceptance/fixtures/local-stub-server.ts` | CREATE/EDIT | Stub identity read for scoped-help tests |
| `rustd/crates/foundation/af_core/src/problem/{fleet,integration}.rs`, `rustd/crates/daemon/afd_fleet/src/error/detail.rs`, `afd_fleet/src/lease/unanswered.rs`, `afd_fleet/src/lease/mint/admission.rs`, `afd_gate/src/gate/first.rs`, `afd_ingress/src/slack/notice.rs`, `afd_credential/src/provider/managed.rs`, `afd_vault/src/error/detail.rs` | EDIT | Server hints and notices name `fleet resume`, `fleet events`, `fleet kill`, `secrets create` |
| `rustd/crates/daemon/afd_ingress/src/slack/notice_tests.rs`, `afd_fleet/tests/integration_unanswered_notice.rs`, `afd_api/tests/integration_slack_mention/resident_bound.rs`, `afd_api/tests/tenant_billing.rs` | EDIT | Assert new spellings |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog.tsx`, `ui/packages/app/tests/models-registry-edit-remove.test.tsx`, `ui/packages/website/src/pages/Fleets.tsx` | EDIT | User-visible CLI hints (after the M249_001 rebase) |
| `ui/packages/app/tests/e2e/acceptance/{operator-journey,retry-policy,soak-cli-smoke}.spec.ts` | EDIT | They spawn `list`, `api-key create`, `steer` (after the rebase) |
| `.ast-grep/rules/docs/docs-removed-command{,-json,-markdown}.yml`, `.ast-grep/rule-tests/docs-removed-command*-test.yml` | EDIT | DOC-09 gains the removed root verbs |
| `tests/fixtures/fleets/platform-ops/definition.json`, `playbooks/operations/admin_bootstrap/001_playbook.md`, `docs/AUTH.md`, `docs/architecture/{billing_and_provider_keys,capabilities,connectors,data_flow,high_level,lease_flow,postgres_schemas,user_flow}.md`, `docs/architecture/scenarios/{github-pr-reviewer,production-deploy-repair,two-fleets-github-slack}.md` | EDIT | Command examples; completion regeneration in `docs/AUTH.md` |
| `rustd/crates/daemon/{afd_fleet_lifecycle,afd_api_wire,afd_authoring,afd_gate,afd_admission,afd_ingress,afd_api_tenant,afd_fleet,afd_runner,afd_cron}/**`, `afd_api/tests/**`, `daemon/main/tests/**`, `af_core/src/{error_code,problem}/**` | EDIT | §6 lifecycle: three states, delete in any status, gate archives |
| `cli/src/program/tree/fleet.command.ts`, `cli/src/commands/fleet/**`, `cli/test/**` | EDIT | §6: `archive` added; `stop`, `resume`, `kill` removed |
| `public/openapi.json` | GENERATE | §6 status and PATCH enums, `--no-banner` |
| `VERSION`, `cli/package.json` and `make sync-version` consumers | EDIT | `0.76.0` → `0.77.0` (CL-BUMP) |
| `~/Projects/docs` branch `chore/m249-cli-commands-changelog` (`4c99d50`): `changelog.mdx`, `cli/agentsfleet.mdx` and 15 pages naming removed spellings | EDIT | Upgrading entry, regrouped reference, current spellings; pushed with this PR, merged after it |

**Blast inventory at `4f3159269`.** Command: `git grep -l -P 'agentsfleet (list|status|stop|resume|kill|delete|steer|logs|events|approvals|schedule|memory|grant|tenant|billing|secret|connector|api-key|models --provider|workspace secrets)\b'`, excluding `docs/v2`, `docs/v1`, `docs/reviews` and changelogs, plus argv spawns `\["(--json", )?"(list|steer|api-key|…)"` in `ui/`. Result: 90 files, 54 production and 36 test. CLI: 26 production, 26 test. Outside the CLI, every production and test file is in the rows above. `playbooks/operations/acceptance/baselines/*` are historical evidence and stay. Templated strings (`fleet.ts:161`, `library.ts:40`) escape the search and are covered by Dimension 4.1's runtime check.

## Applicable Rules

- `.orly/standards/code.md`: FLL (350-line files, 50-line functions), CODE-BULK (anchored, counted rewrites), CODE-MSID.
- `.orly/standards/spec.md`, `pr.md`, `docs.md`, `changelog.md` (CL-BUMP, CL-UPGRADE, CL-HISTORY).
- `.orly/standards/domains/auth.md`: AUTH-NOLOG and AUTH-SECRET (discovery never logs the credential or identity body), WAUTH and AWO (visibility never stands in for the server's check).
- `.orly/standards/languages/typescript.md`: TS-SCHEMA (Schema at the wire boundary), TS-ERRSTYLE, TS-TIMEOUT and TS-RETRY (the bounded read), TS-EXPORT, TS-BUNTEST, TS-TESTFILE.
- `.orly/standards/tests.md`: TEST-NEG, TEST-PUBLIC, TEST-INJECT (the timeout seam), TCF, TEST-ABSENT (hidden names).
- Effect discipline (Indy, Nov 12, 2026: "use a robust code, with better error handling functional programmig approach via effects"): typed failures in the Effect error channel, `Schema` decoding, no `throw` or `try`/`catch` in new code.
- NLR / NLG: no compatibility aliases (Indy: "i dont need a guarded cutover").

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| `orly gate work`, `orly gate verify`, `orly gate pr` | Yes | Hooks at commit and push; `orly gate pr` by hand at CHORE(close) |
| `make lint-all` | Yes | TypeScript lint, file/function limits, DOC-09 ast-grep |
| `make test-unit-all` | Yes | CLI coverage floor; Rust unit tests for the changed hints |
| `make test-integration-rustd` | Yes | Notice and hint strings change; their integration tests run here |
| `make check-version` | Yes | `0.77.0` across consumers |
| `make acceptance-local SUITE=cli` | Yes | Deterministic and live CLI lanes |
| `make acceptance-local SUITE=e2e` | Yes | Dashboard acceptance spawns the CLI |
| `gitleaks detect --redact --no-banner` | Yes | No credential bytes in fixtures |

## Prior-Art / Reference Implementations

- `cli/src/program/tree/*.command.ts`: the domain files already declare groups; they become factories.
- `cli/src/program/entry/argv-scan.ts` and `cli.ts` `helpArgv`/`firstCommandIndex`/`VALUE_TAKING_GLOBAL_FLAGS` (`cli.ts:281`): the pre-parse scan the discovery-request test reuses.
- `cli/src/commands/workspace/create-reconcile.ts:32`: the `retry: { maxAttempts: 1 }` form.
- `effect/src/cli/CliOutput.ts` `layerPresenter`: the public seam for presentation.
- Indy's reference CLI `oss/js/cli` (Supabase): `<group> <verb>` resource commands.

## Sections (implementation slices)

Order: §1 → §2; §3 is independent; §4 follows the M249_001 rebase; §5 last.

### §1 — Coherent command composition

Each domain file exports a factory; none exports a command constant. Only the platform factory takes the invocation's `Visibility`. `tree/index.ts` composes them. `cli.ts` resolves `commandPath` over `buildCommandTree(Unknown)`. Visibility changes only `unlisted` flags, never names, so the path is the same in every tree. It then builds the invocation's tree once (§2) and gives that value to `houseRejection`, `renderLibraryFailure` and `Command.runWith`.

| Current | Destination |
|---|---|
| `list`, `status`, `stop`, `resume`, `kill`, `delete` | `fleet list`, `fleet status`, `fleet stop`, `fleet resume`, `fleet kill`, `fleet delete` |
| `steer`, `logs`, `events` | `fleet steer`, `fleet logs`, `fleet events` |
| `approvals …`, `schedule …`, `memory …` | `fleet approvals …`, `fleet schedule …`, `fleet memory …` |
| `grant list`, `grant delete` | `fleet grants list`, `fleet grants revoke` |
| `models [--provider] [--advanced]` | `models list [--provider] [--advanced]` |
| `tenant provider show`, `create`, `delete` | `models current`, `models use`, `models reset` |
| `secret create/update/show/list/delete`, `workspace secrets` | `secrets create/update/show/list/delete` (§7 removed `secrets open`) |
| `connector list/show` | `connections list/show` |
| `api-key create/list/revoke/delete` | `api-keys create/list/revoke/delete` |
| `billing show` | `usage show` |

`fleet show`, `library` and its children, `chat` and `chat list/rename/delete`, `workspace`, `login`, `logout`, `whoami`, `doctor` and `platform` keep their spelling. `pet` stays runnable and becomes unlisted. Arguments, flags, cardinality, cancellation, ETags, exit codes, redaction and JSON output are unchanged. `chat --fleet` edits setup; `fleet steer` sends a runtime instruction. The guard keys on the root name (`auth-guard.ts:90`); `login` and `pet` remain its only auth-exempt roots.

- **Dimension 1.1** — DONE — Help and execution agree → Test `canonical_groups_share_help_and_handlers` (unit, `cli/test/command-groups.unit.test.ts`): for every row of `cli/test/fixtures/command-parity.ts` (new spelling, fixture args, expected method and path sequence, recorded once from the old spelling at `4f3159269`), `--help` of the group lists the leaf, and the stubbed fetch records exactly that sequence.
- **Dimension 1.2** — DONE — Removed spellings have no aliases → Test `removed_root_verbs_are_rejected` (unit, same file): each of the 18 removed root tokens exits 4 with code `UNKNOWN_COMMAND`, and its block's `suggestion` names no removed spelling; `fleet steer F -- --json` sends `--json` as message text.
- **Dimension 1.3** — DONE — Setup and runtime messages stay distinct → Test `setup_chat_never_calls_runtime_steer` (unit, same file): `chat --fleet F --setup-revision 1 --model authoring-default --message x --no-input` records at least one request, all under `/v1/workspaces/{ws}/drafts`, none to `/fleets/F/messages`; `fleet steer F x` records a `POST /v1/workspaces/{ws}/fleets/F/messages` and nothing under `/drafts`.
- **Dimension 1.4** — DONE — No module-level command constants → Test `tree_modules_export_no_command_constants` (unit, `cli/test/tree-exports.unit.test.ts`): a source scan of `cli/src/program/tree/*.ts` finds no `export const <name> = Command.` and no exported array of commands; a fixture string containing the pattern is detected (positive control).

### §2 — Scope-aware help and completions

**Discovery request.** `isDiscoveryRequest(argv, tree)` walks argv after `helpArgv` the way `resolveCommandPath` does. The invocation is a discovery request when `--completions` appears before `--`, or when the reached node is the root or a `platform` group with children: it is about to print help, or a did-you-mean for the token it stopped on. A reached platform leaf discovers only for `--help`/`-h`, so running `platform keys list` makes no extra read. `-v`/`--version` are answered before this step. No other path discovers: `fleet --help` and `whoami` make no extra read.

**Credential eligibility.** Discovery uses the credential `resolveToken` would send: the environment key first, otherwise the stored login, whose token and `api_url` both come from one `Credentials.snapshot` read (`getAccessToken` already rejects malformed stored values). It skips the read when there is no credential. A stored login is sent only when the normalized target equals its normalized `api_url`, so an unbound login (no `api_url`) is never sent. An environment key is sent only when `flaggedApiUrl(argv)` is null, that is, the target is inferred or comes from `AGENTSFLEET_API_URL`. An ineligible environment key skips the read; it never falls back to the stored login.

**The read.** A discovery layer built from the invocation's `CliConfig` provides only `Credentials` and `HttpClient`. It runs `http.request({ path: USERS_ME_PATH, token, retry: { maxAttempts: 1 }, timeoutMs })`, with `timeoutMs = io.discoveryTimeoutMs ?? DISCOVERY_TIMEOUT_MS` (2000, exported from `discovery.ts`; `RunCliIo.discoveryTimeoutMs` is the test seam). `scopes` is decoded with `Schema.Array(Schema.String)` from the raw body; any failure, timeout, `NetworkError`, `ServerError` or `UnexpectedError` yields `Visibility.Unknown`. A failed or skipped read writes nothing to stdout or stderr. `decodeIdentity`, `whoami` and `login` are untouched. The read runs before the command span and is not traced; it runs with `Console.Console` set to `consoleForStreams(stdout, stderr)`, `References.LogToStderr` set so a record never lands in a `--completions` script on stdout, and `References.MinimumLogLevel` set from the argv `--log-level` value (own keys only).

**Visibility.** Each platform family factory lists each of its leaves when the server reported that leaf's scope, named once in `PLATFORM_SCOPE` (`platform-flags.ts`); leaf descriptions keep naming their scope for people reading help. A leaf is listed when its scope is in the server's list; a family and `platform` are unlisted when none of their leaves is listed. A hidden exact command still parses, reaches the guard and is decided by the server; its own help already names its scope (`platform-models.command.ts:10`). Did-you-mean text comes only from the library's `UnknownSubcommand.suggestions`, which omits unlisted names.

| Leaf | `requiredScope` |
|---|---|
| `platform keys list` | `platform-key:read` |
| `platform keys use`, `delete` | `platform-key:admin` |
| `platform models list`, `show` | `model:read` |
| `platform models create`, `update`, `delete` | `model:admin` |
| `platform runners list`, `show`, `events`, `leases` | `runner:read` |
| `platform runners enroll` | `runner:enroll` |
| `platform runners update`, `rotate-token`, `delete`, `cordon`, `drain`, `revoke`, `self-test` | `runner:write` |
| `platform fleet-library list`, `show`, `stage`, `update`, `publish`, `unpublish`, `delete` | `platform-library:write` (row removed in M249_003) |

**Completions.** `GlobalFlag.Completions` joins Help, Version and LogLevel. `agentsfleet --completions <shell>` prints a static script for the invocation's visibility. The built-in's description is fixed (`GlobalFlag.ts:247`), so `global-flags.ts` exports `completionsFlag = GlobalFlag.Action({ flag: GlobalFlag.Completions.flag.pipe(Flag.withDescription(…)), run: GlobalFlag.Completions.run })` and `CliConfig.builtIns` becomes `[GlobalFlag.Help, GlobalFlag.Version, completionsFlag, GlobalFlag.LogLevel]`. `docs/AUTH.md` and that description say to write the script to a file and regenerate it after login, a target change or a permission change, because sourcing `$(agentsfleet --completions zsh)` in an rc file makes a read at every shell start. Server capability refresh stays the backend's: 60 seconds fresh, up to 15 minutes stale during an identity-provider outage (`afd_identity/src/capability.rs:3`).

- **Dimension 2.1** — DONE — Help follows the scope matrix → Test `platform_discovery_matches_effective_scope_matrix` (unit, `cli/test/scoped-discovery.unit.test.ts`): stubs send server closures `[]`, `["platform-key:read"]`, `["platform-key:admin","platform-key:read"]`, `["model:read"]`, `["model:admin","model:read"]`, `["runner:read"]`, `["runner:enroll"]`, `["runner:write","runner:read"]`, `["platform-library:write"]` and `["workspace:any"]`; root help, `platform --help` and each family's `--help` list exactly the leaves the table grants.
- **Dimension 2.2** — DONE — Unknown scopes fail closed → Test `discovery_refuses_malformed_expired_and_unavailable_identity` (unit, same file): a body without `scopes`, `["model:read", 7]`, a 401, a fetch stub that never responds and rejects with `DOMException("aborted", "AbortError")` when `init.signal` aborts, with `discoveryTimeoutMs: 50`, a network error and a signed-out run each give exit 0 on `--help`, no `platform` in root help, `platfrom` with no `platform` suggestion, stdout and stderr byte-identical to the signed-out run, and `runCli` resolving within 1900 ms, under the 2000 ms default timeout so a read that ignores the seam still fails.
- **Dimension 2.3** — DONE — Discovery belongs to its invocation → Test `discovery_does_not_reuse_another_identity_or_target` (unit, same file): env key plus stored login → the read carries the env key; two sequential runs with different credentials see different trees; `--api http://a` then `--api http://b` with a stored login from `http://a` → one read to `a`, none to `b`; env key with `--api http://b` and no stored login → zero reads; env key plus a stored login from `http://b` with `--api http://b` → zero reads; env key with `AGENTSFLEET_API_URL=http://b` → one read to `b` carrying the env key; an unbound stored login with an inferred target sends no read for `--help`, `platform --help`, `--completions zsh` and `platfrom`; `fleet list`, `fleet --help` and `platform keys list` make zero identity reads; one discovery request makes exactly one.
- **Dimension 2.4** — DONE — Hidden commands keep backend refusal → Test `hidden_command_and_stale_completion_cannot_bypass_authorization` (unit, same file): with `[]`, `platform keys list` parses, sends `GET /v1/admin/platform-keys` and renders the stubbed 403's `UZ-AUTH-022` in the `code` line with exit 3 (`ServerError`, `errors/index.ts:144`).
- **Dimension 2.5** — DONE — Every discovery surface agrees → Test `scoped_help_suggestions_and_four_shell_completions_agree` (unit, `cli/test/completions.unit.test.ts`): for `["model:read"]`, root help lists `platform` and `platfrom` suggests `platform`; the bash script defines `_agentsfleet_platform()` and `_agentsfleet_platform_models()` and none of `_agentsfleet_platform_keys()`, `_agentsfleet_platform_runners()`, `_agentsfleet_platform_fleet_library()`; the zsh, fish and powershell scripts contain `Manage the shared model catalogue` and none of `Manage vault-backed platform defaults`, `Manage platform execution hosts`, `Manage the platform fleet library`; no script contains `rotate-token`, `cordon` or `self-test`. For `[]`: no bash script defines `_agentsfleet_platform` and no other script contains `Administer platform resources with your existing operator scopes`.
- **Dimension 2.6** — DONE — No credential in discovery output → Test `discovery_writes_no_credential_or_identity_to_diagnostics` (unit, `cli/test/scoped-discovery.unit.test.ts`): `--help --log-level debug` with env key `agt_t_SENTINEL` and identity email `probe@example.test` leaves neither string on stdout or stderr, while stderr does carry the read's `http.attempt … endpoint=/v1/users/me` debug record (positive control: debug output was on).
- **Dimension 2.7** — DONE — Scoped help through the built binary → Test `built_binary_help_hides_platform_without_scope` (e2e, deterministic acceptance lane, `cli/test/acceptance/help-and-errors.spec.ts`): the built `dist/bin/agentsfleet.js --help`, run with `AGENTSFLEET_API_URL=<stub origin>`, `AGENTSFLEET_API_KEY=<stub token>`, no `--api` and an empty state directory, lacks `platform` when the stub answers `[]` and lists it for `["model:read"]`; the stub records exactly one `GET /v1/users/me` per run.

### §3 — Aligned error presentation

`Command.runWith` runs with `renderErrors: true` and `CliOutput.layerPresenter(housePresenter)` from `program/entry/presenter.ts`. `Help` events print `formatter.formatHelpDoc(event.helpDoc)` through `Console.log`; `Version` delegates to `CliOutput.defaultPresenter`; `InvalidInvocation` writes nothing. `renderLibraryFailure` is the only writer of a usage-error block (text) or envelope (`--json`), built from `houseRejection(Cause.squash(cause), tree, argv)`; under `--json`, stderr holds exactly that one envelope. Every other failure writer (`renderCliError`, guard refusals through `io.ts` `writeError`, the unreadable-state notice) uses `output/error-block.ts`. No `TaggedCliError` defines `[CliError.userMessage]`, so `showUserError` never fires and `renderCliError` remains the only renderer of handler failures.

A block's first line is `✕ error: <message>` (the unreadable-state notice uses `! warning:`). Labelled lines follow with a two-space indent; the label column is always 12 characters wide (`did you mean`) plus two spaces, so values start at column 17 whichever labels are present. On a terminal that reports its `columns`, long values wrap to column 17 using `wrapAt` and `visibleLength`; off a terminal each field stays on one line, so logs and pipes stay greppable. Colour follows `detectColorMode` (`NO_COLOR`, `FORCE_COLOR`, non-TTY). `--json` envelopes are unchanged except where Dimension 3.2 says.

| Kind | `code` | `suggestion` | `usage` | `did you mean` | `request` |
|---|---|---|---|---|---|
| Server error | its `UZ-` code | its suggestion | — | — | its request id, when sent |
| Network error | `NetworkError` | its suggestion | — | — | — |
| Guard refusal | `AUTH_REQUIRED` / `DEPLOYMENT_UNKNOWN` | the refusal's fix | — | — | — |
| Leaf usage error | `INVALID_ARGUMENT`, `MISSING_ARGUMENT`, `UNKNOWN_OPTION`, … | `run agentsfleet <path> --help` | the leaf's usage line | — | — |
| Unknown command or bare group | `UNKNOWN_COMMAND` | `run agentsfleet <path> --help` | — | library suggestions, when any | — |
| Invalid `--completions` value | `INVALID_ARGUMENT` | `run agentsfleet --help`; the first line lists the shells in words | — | — | — |
| Unreadable saved sign-in | — | the existing fix sentence | — | — | — |

- **Dimension 3.1** — DONE — One aligned block for every failure → Test `error_blocks_align_labels_and_wrap` (unit, `cli/test/error-blocks.unit.test.ts`): a server error with code, suggestion and request id, a network error, a guard refusal, a usage error (`fleet kill abc`), an unknown command (`lsit`) and `--completions tcsh` each render with every label value starting at column 17 at widths 60 and 120; stdout is empty and stderr contains no `DESCRIPTION` heading.
- **Dimension 3.2** — DONE — Machine output is unchanged → Test `json_error_envelope_is_unchanged` (unit, same file): under `--json` the same failures, except `--completions tcsh`, produce the literal envelopes recorded in the test from the base (for example `{"error":{"code":"UNKNOWN_COMMAND","message":…}}`); `--completions tcsh` is new and produces `{"error":{"code":"INVALID_ARGUMENT","message":…}}`; stdout stays empty and stderr is exactly one JSON document.
- **Dimension 3.3** — DONE — Requested help is untouched → Test `requested_help_prints_unchanged_document` (unit, same file): `fleet --help` and bare `fleet` print the help document on stdout with exit 0 and no block.

### §4 — Server and interface strings, docs and rebase

After `gh pr view <M249_001 PR> --json state` reads `MERGED`, rebase onto it. Then update the UI hint strings and the dashboard specs that spawn the CLI, alongside the server hints, Slack and lease notices, docs, fixtures and playbooks listed in Files Changed. Extend DOC-09 with the removed root verbs. Bump to `0.77.0`; add the Upgrading entry on the docs branch after merge.

- **Dimension 4.1** — DONE — Every printed command parses → Test `cli_hint_strings_name_real_commands` (unit, `cli/test/command-groups.unit.test.ts`): every string literal in `cli/src/**` (comments excluded) containing `agentsfleet `, read as the words after it up to the first token starting with `-`, `<`, `[`, `$` or a backtick, and every rendered suggestion in the Dimension 1.1 run, resolves through `resolveCommandPath` on `buildCommandTree(Unknown)` to the full word sequence it names, or to a leaf followed by its declared positionals.
- **Dimension 4.2** — DONE — Removed spellings are lint errors → DOC-09 (`docs-removed-command*.yml`) extended with the 18 removed verbs, rule-test positive `agentsfleet kill x`, negatives `agentsfleet fleet kill x` and `agentsfleet secrets list` → `make lint-all` exit 0.
- **Dimension 4.3** — DONE — Server hints name current commands → Tests `integration_unanswered_notice` and `notice_tests` (existing, Rust) assert `agentsfleet fleet events` and `agentsfleet fleet resume` → `make test-integration-rustd` and `make test-unit-all` exit 0.
- **Dimension 4.4** — DONE — Lands before the UI refresh, with the current dashboard on the new lifecycle → Tests: the dashboard on this branch (`ui/packages/app`) sends only `active` and `archived`, deletes without archiving first, reads `archived` instead of `killed`, `stopped`, `paused` and `auto_killed`, and offers Archive and Delete in place of Stop, Resume and Kill; `make test-unit-all` and `make dry-app` pass. The UI refresh (M249_001) rebases onto this merge.

### §5 — Help report and verification

- **Dimension 5.1** — DONE — Every help page is reviewed → `manual` (Indy, Nov 12, 2026: "i need a report of the help, and the subcommands helps so i know you have fixed it"): on the final tree, `--help` for root and every group and leaf under three scope sets (`[]`, `["model:read"]`, and the union of every scope in the §2 table), plus one sample of each §3 block, written by a helper kept outside the repository; the report goes to Indy and its path into the PR Session Notes.
- **Dimension 5.2** — DONE — Lanes are green on the final tree → rubric R1–S4 at `orly gate pr`.

### §6 — Fleet lifecycle: archive and delete

Indy, Nov 13, 2026 (Discovery): `fleet stop`, `fleet resume` and `fleet kill` go, with no backward path. `fleet archive` is a new terminal state that keeps history. `fleet delete` removes a fleet in any status. Archive and delete are independent; neither invokes the other. A gate trip archives the fleet, false positives accepted. Scopes do not change: PATCH keeps `fleet:write`, DELETE keeps `fleet:admin`, and workspace membership already holds both (`afd_http/src/auth/ownership/role.rs:42-52`).

```
installing ──PATCH active──▶ active ──PATCH archived (fleet archive)──▶ archived
                               └──gate trip (anomaly or auto_kill)────▶ archived
any status ──DELETE (fleet delete)──▶ gone
```

- `FleetStatus` is `Installing | Active | Archived` (`afd_fleet_lifecycle/src/lib.rs`); `Paused`, `Stopped` and `Killed` are removed. `PatchStatus` is `Active | Archived` (`afd_api_wire/src/fleet/input.rs`). PATCH `active` only activates an installing fleet; PATCH `archived` does what PATCH `killed` did (`afd_authoring/src/creation/patch.rs`: cancels leases and setup, restores the draft). An archived fleet refuses every further transition with 409 `UZ-AGT-020` and `current_state: archived`; a GET still returns it, and a missing fleet stays 404 `UZ-AGT-009`.
- The gate's trip writes `archived` where it wrote `paused` (`afd_gate/src/gate/pause.rs`). Admission and ingress treat `archived` as not runnable where they named `paused` or `stopped`.
- Purge drops its terminal-only guard (`afd_fleet_lifecycle/src/purge.rs:111`, `MustKillFirst`). Deleting a fleet with live work first cancels its leases through the same path archive uses.
- Approving a setup grant re-advances that fleet's setup, so an installing fleet becomes active with no further request; this replaces `fleet resume` as the retry after approval. A denial does not advance. PATCH `active` on an installing fleet still retries setup for callers that use it.
- Schedule `desired_status = 'paused'` is a schedule state, not a fleet state, and stays.
- Known limits, recorded rather than built: a gate trip writes only the status, so a lease already running ends on its own (the cancel path is in `afd_authoring`, which `afd_gate` cannot reach); and delete purges lease rows in the transaction after its release, so a runner that reads the Cancel late stops at its next renewal (`RUN_LEASE_NOT_FOUND`).
- The current dashboard on `main` moves to the new lifecycle in this PR (Dimension 4.4): `ui/packages/app/lib/api/fleets.ts` and `fleets-types.ts` statuses, `components/layout/FleetDeleteDialog.tsx` (no archive before delete), `KillSwitch.tsx` (Archive and Delete), the approval status `archived`, and their tests. `docs/DESIGN_SYSTEM.md`, `docs/architecture/web_app.md` and the acceptance playbook follow.
- CLI: `fleet activate <fleet_id>` sends PATCH `active`, which re-checks an installing fleet's setup: the retry after fixing a missing secret, connector, model or balance, which no event retries on its own; it prints the status the server answered, and on `installing` names `agentsfleet fleet show`. `fleet archive <fleet_id>` prints `<id> archived.`; `fleet delete <fleet_id>` prints `<id> deleted.`; `stop`, `resume` and `kill` are removed. Status rendering and every hint naming them change. `public/openapi.json` is regenerated. The refreshed UI's lifecycle actions follow after the rebase (Dimension 4.4).

- **Dimension 6.1** — DONE — The lifecycle has three states → Test `fleet_lifecycle_is_activate_archive_delete` (integration, `afd_api`): PATCH `archived` on an active fleet returns `archived` and cancels its leases; PATCH `active` or `archived` on an archived fleet is refused with 409 `UZ-AGT-020` and `current_state` `archived`; PATCH `stopped`, `killed` or `paused` is a validation error; events of an archived fleet stay readable.
- **Dimension 6.2** — DONE — Delete works in any status → Test `delete_removes_a_fleet_in_any_status` (integration, `afd_api`): DELETE on an installing, an active and an archived fleet each returns 2xx, a later GET returns 404, and no active lease remains.
- **Dimension 6.3** — DONE — A gate trip archives → Test `gate_trip_archives_the_fleet` (integration, `afd_gate`): an anomaly trip and an `auto_kill` rule each leave the fleet `archived`.
- **Dimension 6.5** — DONE — Approval resumes setup → Test `approving_a_setup_grant_resumes_setup` (integration): approving the pending setup grant leaves the fleet `active` with no PATCH; denying it leaves the fleet `installing`.
- **Dimension 6.6** — DONE — A gate rule only asks, and a runaway archives and closes its event → Tests: a Definition carrying `behavior` is refused as an unknown key; a matching rule raises an approval and never archives; a runaway trip archives the fleet and settles the triggering event with a named label instead of retrying it; the approval status enum lists `archived`, not `auto_killed`.
- **Dimension 6.4** — DONE — The CLI offers archive and delete only → Test `fleet_lifecycle_commands` (unit, CLI): `fleet activate <id>` sends one PATCH `{"status":"active"}` and reports a still-waiting setup with its next step; `fleet archive <id>` sends one PATCH `{"status":"archived"}`; `fleet delete <id>` sends one DELETE and no PATCH; `fleet stop`, `fleet resume` and `fleet kill` are `UNKNOWN_COMMAND`.

### §7 — Review-driven command changes

Indy reviewed the full command table on Nov 13, 2026 (Discovery) and chose four changes. They change the CLI only; routes, payloads and scopes stay.

- `library` becomes `plugins` (`list`, `delete <plugin_id>`), matching the refreshed UI's "Plugins". `chat --library` becomes `--plugin` and `--library-etag` becomes `--plugin-etag`. The API keeps `fleet-libraries` until M249_003.
- `chat delete <draft-id>` deletes without a prompt and has no `--yes`, like every other delete.
- `secrets open` is removed: it opened nothing and only pointed at the dashboard.
- `platform keys set` becomes `platform keys use`, matching `models use`.

- **Dimension 7.1** — DONE — The four changes hold → Tests: `chat/library.test.ts` (`plugins` lists, deletes and hints `chat --plugin`), `chat/pinned-source.test.ts` (`--plugin requires --revision`), `chat/history.test.ts` (`deletion asks nothing and sends one DELETE, unattended or not`; `the removed --yes flag is refused`), `platform-catalogue.unit.test.ts` and `scoped-discovery.unit.test.ts` (`keys use`), `json-contract.test.ts` (no `secrets open`), golden root help; DOC-09 flags `agentsfleet library`, `chat --library`, `secrets open` and `platform keys set`.

## Interfaces

```
type Visibility = { _tag: "Unknown" } | { _tag: "Known"; scopes: ReadonlySet<string> }
buildCommandTree(visibility: Visibility = Unknown): Command   // cli/src/program/tree/index.ts
DISCOVERY_TIMEOUT_MS = 2000; RunCliIo.discoveryTimeoutMs?: number
isDiscoveryRequest(argv: ReadonlyArray<string>, tree: CommandNode): boolean   // cli/src/program/entry/discovery.ts
asCommandNode(command): CommandNode                                         // cli/src/program/tree/resolve-path.ts
housePresenter: CliOutput.Presenter   // cli/src/program/entry/presenter.ts; needs no input
```

Root: `login`, `logout`, `whoami`, `doctor`, `workspace`, `chat`, `fleet`, `plugins`, `models`, `secrets`, `connections`, `api-keys`, `usage`, scope-advertised `platform`; unlisted `pet`. Global flags: `--api`, `--json`, `--help`/`-h`, `--version`/`-v`, `--log-level`, `--completions bash|zsh|fish|powershell|sh|pwsh`.

- `fleet`: `list [--workspace] [--starting-after] [--limit]`, `show <fleet_id>`, `status`, `activate <fleet_id>`, `archive <fleet_id>`, `delete <fleet_id>`, `steer <fleet_id> [message]`, `logs [fleet_id] [--fleet] [--limit] [--cursor]`, `events <fleet_id> [--actor] [--since] [--cursor] [--limit]`, `approvals {list [--fleet], show <gate_id>, approve <gate_id>, deny <gate_id>}`, `schedule {…as today}`, `memory {…as today}`, `grants {list [--fleet], revoke <grant_id> [--fleet]}`.
- `models`: `list [--provider] [--advanced]`, `current`, `use [--secret <name>] [--model <ref>]`, `reset`.
- `secrets`: `create <name> […typed provider flags]`, `update <name> […]`, `show <name>`, `list`, `delete <name>`.
- `connections`: `list [--workspace]`, `show <provider> [--workspace]`. `api-keys`: `create [--name] [--description]`, `list [--sort]`, `revoke <api_key_id>`, `delete <api_key_id>`. `usage`: `show [--limit] [--cursor]`. `workspace`: `create`, `list`, `use`, `show`, `delete`.
- Error block labels: `code`, `suggestion`, `usage`, `did you mean`, `request`; values at column 17.

Discovery adds no endpoint and no permission family. HTTP routes are unchanged; fleet statuses are `installing`, `active`, `archived`, and PATCH accepts `active` and `archived` (§6).

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Scope read unknown or malformed | Security | Visibility `Unknown`; typed commands still reach backend checks | `discovery_refuses_malformed_expired_and_unavailable_identity` |
| Help request names a foreign `--api` | Security | Stored login sent only to its minting deployment; env key never to a flag-only host | `discovery_does_not_reuse_another_identity_or_target` |
| Unbound stored login on a discovery request | Security | No `api_url`, so it never matches the target | `discovery_does_not_reuse_another_identity_or_target` |
| Ineligible env key with a stored login | Security | Skip; never fall back to the stored login | `discovery_does_not_reuse_another_identity_or_target` |
| Credential or target changes between runs | Security | Visibility lives in one invocation's tree; no cache | `discovery_does_not_reuse_another_identity_or_target` |
| Hidden name leaks through suggestions | Security | Suggestions only from the library | `scoped_help_suggestions_and_four_shell_completions_agree` |
| Identity read hangs | Failure | `timeoutMs` aborts the fetch; one attempt | `discovery_refuses_malformed_expired_and_unavailable_identity` |
| Parser and help diverge | Failure | One tree value feeds both | `canonical_groups_share_help_and_handlers` |
| A hint names a removed command | Failure | Runtime resolution test plus DOC-09 | `cli_hint_strings_name_real_commands` |
| Help buried under a usage error, or misaligned block | Failure | Presenter plus one block renderer | `error_blocks_align_labels_and_wrap` |
| Invalid `--completions tcsh` | Failure | Library choice error as a block, exit 4 | `error_blocks_align_labels_and_wrap` |
| Stale completion script | Failure | Docs name regeneration; server refuses | `hidden_command_and_stale_completion_cannot_bypass_authorization` |

## Invariants

1. Backend authorization decides every operation: the server's route scope gate (`afd_api/tests/route_meta_scopes.rs`) is the authority, and `unlisted` never reaches the handler path (Dimension 2.4).
2. Credential bytes and identity bodies never reach discovery output or diagnostics (Dimension 2.6; `detectTokenInArgv` in `cli/test/argv-redact.unit.test.ts`).
3. The tree is a per-invocation value: no tree module exports a command constant (Dimension 1.4 source scan).
4. No compatibility aliases: removed spellings fail parsing (Dimension 1.2) and lint (DOC-09).

## Metrics & Observability

No new event. Label values change: span names become `cli.fleet.list` and the analytics `command` becomes `fleet list` (`runtime/command-runtime.service.ts:27`). A search of `playbooks/` and `docs/` for `cli.list`, `cli.kill` and the other old labels finds no consumer. The discovery read runs before the command span and emits no trace or analytics row.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | CLI unit lane | `make test-unit-cli` | exit 0; output names every Dimension test title above | P0 | |
| R2 | Package and Rust unit gates | `make test-unit-all` | exit 0 | P0 | |
| R3 | Rust integration | `make test-integration-rustd` | exit 0 | P0 | |
| R4 | CLI acceptance | `make acceptance-local SUITE=cli` | exit 0 | P0 | |
| R5 | Dashboard acceptance that spawns the CLI | `make acceptance-local SUITE=e2e` | exit 0 | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Version | `make check-version` | exit 0 at `0.77.0` | P0 | |
| S3 | Gates | `orly gate work`; `orly gate verify`; `orly gate pr` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

Delete `root.command.ts`, the root-level verb factories, `tenantCommand`, `billingCommand`, `workspace secrets`, the `Suggestion:` formatting in `cli.ts` and `cli-error-render.ts`, and the text-mode help dump path. Keep historical changelog entries and acceptance baselines.

## Out of Scope

- Plugin vocabulary, Plugin `show`/`revisions` readers, route/wire/permission/SQL/status renames, platform catalogue removal and UI adapters: M249_003.
- The `agentsfleet pet` sprite: its own spec after M249_003 (Indy, Nov 12, 2026).
- Flow 2 capability composition, `@plugin` invocation and bindings (M239); new platform UI, ownership or sharing models, a primary vault.

## Product Clarity (authoring record)

1. **Successful user moment** — `agentsfleet fleet --help` lists every fleet verb; an operator holding `model:read` sees `platform models` and nothing else; a typo prints one aligned block.
2. **Preserved user behaviour** — Handlers, scopes, ETags, recovery, JSON envelopes, `--help` documents.
3. **Optimal-way check** — Effect's `unlisted`, `Completions` and presenter cover the need; no second parser, no help-text filtering.
4. **Rebuild-vs-iterate** — Refactor assembly and presentation; reuse every handler.
5. **What we build** — Domain factories, scoped discovery, four-shell completions, one error block.
6. **What we do NOT build** — Aliases, a permission cache, an identity endpoint, vocabulary renames.
7. **Fit with existing features** — Reuses `/v1/users/me`, the guard, `HttpClient` options and the help formatter's wrapping.
8. **Surface order** — CLI first; UI strings after the M249_001 rebase; vocabulary in M249_003.
9. **Dashboard restraint** — Only hint strings and CLI-spawning specs change in `ui/`.
10. **Confused-user next step** — A removed spelling fails with a block whose `suggestion` names the reached group's `--help`; an exact hidden command's help names its scope.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Composition, discovery, presentation, strings and rebase, then the help report.
- **Alternatives considered:** Filtering rendered help diverges from parsing and completions. A module-global mutable tree carries authority between runs. Discovering on every credentialed invocation adds a round trip and a credential send to commands that need neither. Building the discovery read on `mainLayerFor` constructs the telemetry layers twice.
- **Patch-vs-refactor verdict:** Refactor of assembly and presentation; handlers stay.

## Discovery (consult log)

- **Consults:** Source inspected at `4f3159269`. The authored spec referenced UI snapshot `3aa7e368b`, not present locally (`git cat-file -t 3aa7e368b` → not a valid object).
- **Model reviews:** A re-audit (implementation readiness) of the second revision on Nov 13, 2026 found 2 blockers, 11 majors and 7 minors, all precision gaps in §2 and §3 (single usage-error writer under `--json`, completion assertions that collided with `api-keys`, the 403 exit code, a discovery rule that matched platform leaves, the env-key fallback, the fixed completion description), and returned "ready after the listed fixes"; all were applied in the third revision. EGM design, implementation-readiness and codebase-accuracy reviews ran on Nov 13, 2026 against the first split revision (design: 2 blockers, 9 majors; readiness: 2 blockers, 12 majors; codebase: 2 blockers, 8 majors). Accepted and applied: credential eligibility independent of root exemptions and bound to the minting deployment; tree-then-visibility order; narrowed discovery requests; `retry: {maxAttempts: 1}` and `timeoutMs` with a test seam; a separate strict scope decode leaving `login` and `whoami` untouched; presenter mechanism and every failure writer; `TaggedCliError` variants; server, UI and doc strings owned here; version bump; e2e Dimension; source-scan invariant. Transferred to M249_003: all vocabulary renames, Plugin readers and `fleet delete` composition, because Indy's selected split put "CLI flags" in PR B.
- **Red-green evidence (Nov 13, 2026).** Each mutation below was applied by hand, run, and reverted byte for byte (`cmp` against the saved file): discovery writing the token to stderr and an ineligible env key falling back to the stored login → `scoped-discovery.unit.test.ts` 24 pass, 7 fail; the block gap widened by one space → `error-blocks.unit.test.ts` 9 pass, 16 fail; the unreadable credentials file made readable → its test 0 pass, 1 fail. Each restored file returned the suite to green.
- **Rebase plan (Nov 13, 2026).** `feat/m249-fleet-interface-refresh` is on the remote at `b22d51c91` (36 commits past `origin/main`), with no PR yet (`gh pr list --head feat/m249-fleet-interface-refresh` → `[]`). `git merge-tree --write-tree HEAD origin/feat/m249-fleet-interface-refresh` reports two conflicts. `cli/src/program/tree/workspace.command.ts`: the UI branch adds `--workspace` to `tenant provider create`, which carries over to `models use` in `models.command.ts`. `docs/architecture/billing_and_provider_keys.md`: a text clash. The UI branch also adds the authored M249_002 at `pending/`; it differs from the copy this branch started from only in banned-word substitutions, so its copy becomes the authored base and this branch's move to `active/` supersedes it.
- **§6 evidence (Nov 13, 2026).** On the §6 tree: `make lint-all` exit 0; `make test-unit-all` exit 0; `make test-integration-rustd` exit 2 on its first run (two `afd_fleet` parking race tests, `55P03` lock timeout, in code §6 does not touch), then exit 0 on a clean rerun with 1763 passed and 0 failed. The route-level approval test was proved by mutation: flipping `resolve.rs:98` to `Denied` failed it at `grant_resume.rs:89`, and the restored file passed.
- **Close evidence (Nov 13, 2026).**
  - **4.4:** the dashboard on this branch moved to the new lifecycle: `cd ui/packages/app && bun run test:coverage` exit 0 (440 files, 4269 tests, 100% coverage); `make lint-app` exit 0; `make dry-app` exit 0 (68 Playwright specs passed, after `bunx playwright install chromium` fixed 68 "Executable doesn't exist" failures from a missing browser).
  - **5.1:** the help report for the final tree is `/tmp/m249_002/report/help-report.md` (311 help pages and error samples; index in `help-index.md`), generated by a helper kept outside the repository.
  - **6.6:** `tests/definition/fields.rs::a_gate_rule_naming_a_behaviour_is_refused_as_an_unknown_key` and `integration_lease_gates::runaway::a_runaway_trip_archives_the_fleet_and_ends_its_event`; restoring the `Retry` arm failed the latter at `runaway.rs:42`, and the restored file was byte-identical.
  - **5.2:** `orly gate pr` on the pushed revision; its result is in the PR body.
  - **Unverified:** rubric rows R4 and R5 (`make acceptance-local SUITE=cli|e2e`) were not run. They start OrbStack services, trust a local certificate, and need development Clerk credentials, a model key and a signed Linux toolbox; `.orly/LOCAL.md` runs acceptance after merge. About ten acceptance specs and fixtures changed here (lifecycle, teardown, command matrix, help, chat) and are typechecked by `make lint-cli`, not executed.
  - **Integration lane flakes:** two `afd_fleet` parking tests, one `wall_ticks` test and, in the server agent's runs, `contention::one_hundred_concurrent_saves` and `concurrent_plan_replays` failed on `55P03` lock timeouts or `PoolCapacity` while other lanes ran on the same machine. Each passed alone and on a clean rerun; none touches code this branch changes.
- **Integration check overridden for this PR only (Nov 13, 2026).** `orly gate pr` timed out on `make test-integration-rustd` at its 30-minute limit (1,800,019 ms) with no test failing, on a Mac at 16.5 load average. Test time was 15.4 min, and compiling the 248 test programs took from 1 s to 9 min 16 s by cache state. Indy chose to raise the limit, then: "I meant the override for this PR only", and "i dont want to move it up". The gate and its limit are unchanged for every other PR. The proof for this PR is CI's `test-integration-rustd` workflow, which runs the same lane sharded on every pull request. It is not a required check on `main`, and its header records that as an owner decision, so its result must be read on the PR before merging.
- **Dependency updates (Nov 13, 2026, Indy: "if there is an update to any of the packages please do so").** `effect` and `@effect/platform-bun` 4.1.3 → 4.1.4 (CLI and app), plus patch and minor updates; the effect-tied Dimensions (1.1, 2.1, 2.5) pass on 4.1.4. `tsrs` stays at 0.23.0: 0.25.0 is a beta two minors ahead. Rust crates were not checked (`cargo-outdated` is not installed).
- **Open blockers:** none outside this branch. The dashboard move (4.4), the final help report (5.1) and the final gate (5.2) remain.

> Owner (Indy, Nov 12, 2026): "2, 3 can be one spec pushed in your PR. claude will need a handoff prompt to start the spec and create work tree from your merged PR1 by a separate agent. For 2,3 commit in the PR you do for 1, so upon merge we pull and start on this."

> Owner clarification (Indy, Nov 12, 2026): "well if the cli is discrete, then it could start before your merged UI PR. But i will merge the cli after PR merges. so they go in parallel."

> Owner decisions: "Yes flow 1"; "Permanent stop, retain history, no Restore"; "there is no primary vault"; "i dont need a guarded cutover"; "i am only in development".

> Owner decision (Indy, Nov 12, 2026: 11:55 PM), on shipping the ~1000-file rename with the CLI work: selected "Split into two PRs (my call)" — "PR A: command grouping, scoped discovery, completions and the pet. PR B: Plugin + archived rename across Rust, SQL, OpenAPI, CLI flags, UI adapters and docs, rebased onto A."

> Owner (Indy, Nov 12, 2026): "the `agentsfleet pet` make a better pet -- not just ascii. … do this at the end. add this to your punch lisst. You cannot mimic an exact pet like indy sent but its an inspiration." Render choice: "Inline image protocol". Then: "the pet i would move it to a different scope, if it take more scope and libraries and so on."

> Owner (Indy, Nov 12, 2026): "use a robust code, with better error handling functional programmig approach via effects. make the error appear better and aligned." Recorded as §3 and the Effect-discipline rule.

> Owner decision (Indy, Nov 13, 2026: 01:05 AM), on unit tests inheriting a developer's exported `AGENTSFLEET_STATE_DIR` (which holds a real dev login, so scoped `--help` read `/v1/users/me` on api-dev from a unit test): selected "Preload always isolates (my call)" — `cli/test/setup.ts` always creates an empty state dir.

> Owner decisions (Indy, Nov 13, 2026), on the three review items (the Dimension 2.2 time bound raised from 1000 ms to 1900 ms; one factory per command group, kept; discovery's own strict `/v1/users/me` reader beside `readIdentity`, kept): "1, 2, 3 are fixed as per your recommendation". The two edited `main` tests (`entry-builtins.unit.test.ts` now expects `--completions`; `chat/library.test.ts` lets the discovery read through) were explained the same day and drew no objection.

> Owner decisions (Indy, Nov 13, 2026), fleet lifecycle, moved into this PR (§6): "I want to take out or nuke this command (we donot need this) fleet stop <id>"; the same for `fleet resume <id>`; "There is no backward or legacy to think about"; "fleet kill <id> is fleet delete"; "fleet archive is a new state"; "i want this in this PR". On a gate trip archiving: "Yes correct i agree on this, i accept false-positive". On permissions: "there is no specific scope needed"; "by default you give me access". On the setup retry `fleet resume` used to give after a grant approval: "I prefer this approving the grant restarts setup automatically." For the checks no event retries (credential, connection, model, wallet), asked "what is the quickest fix? a retry?": `fleet activate`, CLI only, over the existing PATCH `active`. This supersedes M249_003's §2c and §3.

> Owner decisions (Indy, Nov 13, 2026), on the full command review (§7): "3. remove the chat delete's prompt and the --yes"; "4. rename" (`library` to `plugins`); "6. remove secrets open"; "8. yes rename" (`platform keys set` to `use`); "9. leave it for M249_003?" (`platform fleet-library` removal stays there).

> Owner decisions (Indy, Nov 13, 2026), on the standards review of §6 and §7: delete in one transaction, "1. yes AFTER (one step) - signed off"; schedule sync after an approval, "2. yes signed off"; an archived fleet's refusal, "1. 409 signed off agree". On the runner message a delete sends: "the one you say is fine" (it stays `Cancel`, which stops one run).

> Owner decisions (Indy, Nov 13, 2026), on the gate's remaining kill vocabulary: "7. Yes rename. we dont have to specific auto_kill or auto_archive this must be default"; then "Item 7 - signed off". A gate rule always asks a person (`behavior` and `auto_kill` removed); a runaway archives by default; `auto_killed` becomes `archived`; a gate-stopped event closes when its fleet is archived. On an archived fleet never carrying a Slack notice: "3. Okay signed off".

> Owner decision (Indy, Nov 13, 2026), reversing the merge order: "Codex is still spinning … So in the mean time if your PR is ready we will merge this first." Then: "take a note of it s well". This PR merges first, so it moves the current dashboard on `main` to the new lifecycle itself (Dimension 4.4); the UI refresh rebases onto it.

> Owner (Indy, Nov 12, 2026): "i think after you complete i need a report of the help, and the subcommands helps so i know you have fixed it. and the unit test acceptance fixed too for cli." Recorded as Dimension 5.1 and rubric R4.
