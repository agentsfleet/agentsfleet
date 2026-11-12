# M249_002: Fleet commands and scoped discovery

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 002
**Date:** Nov 13, 2026
**Status:** IN_PROGRESS
**Priority:** P1, make the command surface coherent and advertise only usable operator commands
**Categories:** CLI, DOCS
**Batch:** B2
**Depends on:** M249_001 (UI refresh) merges before this workstream merges; building starts before it
**Provenance:** Agent-generated from Indy's approved CLI grouping and permission-discovery decisions; split from the authored M249_002 on Nov 12, 2026 (see Discovery)
**Canonical architecture:** `docs/architecture/api_resources.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** Every command sits under its resource group, help, suggestions and completions advertise only the platform commands the caller's server-reported scopes allow, and every failure renders as one aligned block, while each typed command keeps its handler and backend authorization.

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
| `docs/v2/pending/M249_003_P1_API_CLI_UI_DOCS_PLUGIN_AND_ARCHIVED_VOCABULARY.md` | EDIT/MOVE | Takes the vocabulary renames and delete composition |
| `cli/src/cli.ts` | EDIT | Discovery step, one visibility tree, presenter, completions |
| `cli/src/program/tree/root.command.ts` | DELETE | Mixed assembly and leaf-handler module |
| `cli/src/program/tree/index.ts` | CREATE | `buildCommandTree(visibility)` |
| `cli/src/program/tree/global-flags.ts` | CREATE | `--api`, `--json` |
| `cli/src/program/tree/auth.command.ts` | CREATE | `login`, `logout`, `whoami` |
| `cli/src/program/tree/diagnostics.command.ts` | CREATE | `doctor`; `pet` unlisted |
| `cli/src/program/tree/{fleet,access,workspace,chat,chat-history,schedule,memory}.command.ts` | EDIT | Factories; `fleet` nests lifecycle, approvals, grants, schedule, memory |
| `cli/src/program/tree/{models,secrets}.command.ts` | CREATE | Moved out of `fleet.command.ts` and `workspace.command.ts` |
| `cli/src/program/tree/platform*.command.ts` (5 files) | EDIT | Per-leaf `requiredScope`; parents unlisted when empty |
| `cli/src/program/tree/flags.ts` | EDIT | Flag descriptions naming moved commands (`flags.ts:157`) |
| `cli/src/program/entry/discovery.ts` | CREATE | `isDiscoveryRequest`, eligibility, bounded read, strict decode |
| `cli/src/program/entry/presenter.ts` | CREATE | `CliOutput` presenter: help passes through, usage errors become blocks |
| `cli/src/output/error-block.ts` | CREATE | Label column, hanging-indent wrap, colour mode |
| `cli/src/lib/cli-error-render.ts`, `cli/src/lib/run-effect.ts`, `cli/src/program/io.ts`, `cli/src/program/entry/rejection.ts`, `cli/src/program/entry/deferred-help.ts` | EDIT | Every failure writer uses the block |
| `cli/src/errors/base.ts`, `cli/src/errors/index.ts` | EDIT | New `TaggedCliError` variants join `CliError` and `EXIT_CODE` |
| `cli/src/commands/{fleet,approvals,grant,secret,schedule,connector,api-key,models,tenant,workspace,billing}/*.ts`, `cli/src/lib/model-catalogue.ts` | EDIT | Hint strings naming moved commands, including templated `agentsfleet ${verb}` (`fleet.ts:161`) |
| `cli/test/**` (52 files with removed spellings), `cli/test/acceptance/**` incl. `fixtures/command-matrix.ts` | EDIT | New spellings |
| `cli/test/{command-groups,scoped-discovery,completions,error-blocks,tree-exports}.unit.test.ts`, `cli/test/fixtures/command-parity.ts` | CREATE | Dimension proofs |
| `rustd/crates/foundation/af_core/src/problem/{fleet,integration}.rs`, `rustd/crates/daemon/afd_fleet/src/error/detail.rs`, `afd_fleet/src/lease/unanswered.rs`, `afd_fleet/src/lease/mint/admission.rs`, `afd_gate/src/gate/first.rs`, `afd_ingress/src/slack/notice.rs`, `afd_credential/src/provider/managed.rs` | EDIT | Server hints and notices name `fleet resume`, `fleet events`, `fleet kill`, `secrets create` |
| `rustd/crates/daemon/afd_ingress/src/slack/notice_tests.rs`, `afd_fleet/tests/integration_unanswered_notice.rs`, `afd_api/tests/integration_slack_mention/resident_bound.rs`, `afd_api/tests/tenant_billing.rs` | EDIT | Assert new spellings |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog.tsx`, `ui/packages/app/tests/models-registry-edit-remove.test.tsx`, `ui/packages/website/src/pages/Fleets.tsx` | EDIT | User-visible CLI hints (after the M249_001 rebase) |
| `ui/packages/app/tests/e2e/acceptance/{operator-journey,retry-policy,soak-cli-smoke}.spec.ts` | EDIT | They spawn `list`, `api-key create`, `steer` (after the rebase) |
| `.ast-grep/rules/docs/docs-removed-command{,-json,-markdown}.yml`, `.ast-grep/rule-tests/docs-removed-command*-test.yml` | EDIT | DOC-09 gains the removed root verbs |
| `tests/fixtures/fleets/platform-ops/definition.json`, `playbooks/operations/admin_bootstrap/001_playbook.md`, `docs/AUTH.md`, `docs/architecture/{billing_and_provider_keys,capabilities,connectors,data_flow,high_level,lease_flow,postgres_schemas,user_flow}.md`, `docs/architecture/scenarios/{github-pr-reviewer,production-deploy-repair,two-fleets-github-slack}.md` | EDIT | Command examples; completion regeneration in `docs/AUTH.md` |
| `VERSION`, `cli/package.json` and `make sync-version` consumers | EDIT | `0.76.0` → `0.77.0` (CL-BUMP) |
| `~/Projects/docs` branch `chore/m249-cli-commands-changelog`: `changelog.mdx` and CLI reference | EDIT | Upgrading entry and reference, after this PR merges |

**Blast inventory at `4f3159269`.** Command: `git grep -l -P 'agentsfleet (list|status|stop|resume|kill|delete|steer|logs|events|approvals|schedule|memory|grant|tenant|billing|secret|connector|api-key|models --provider|workspace secrets)\b'`, excluding `docs/v2`, `docs/v1`, `docs/reviews` and changelogs, plus argv spawns `\["(--json", )?"(list|steer|api-key|…)"` in `ui/`. Result: 90 files, 54 production and 36 test. CLI: 26 production, 26 test. Outside the CLI, every production and test file is in the rows above. `playbooks/operations/acceptance/baselines/*` are historical evidence and stay. Templated strings (`fleet.ts:161`, `library.ts:40`) escape the search and are covered by Dimension 4.1's runtime check.

## Applicable Rules

- `.orly/standards/spec.md`, `pr.md`, `docs.md`, `changelog.md` (CL-BUMP, CL-UPGRADE, CL-HISTORY).
- `.orly/standards/domains/auth.md`: AUTH-NOLOG and AUTH-SECRET (discovery never logs the credential or identity body), WAUTH and AWO (visibility never stands in for the server's check).
- `.orly/standards/languages/typescript.md`: TS-SCHEMA (Schema at the wire boundary), TS-ERRSTYLE and TS-ERRNAME (new `TaggedCliError` variants), TS-TIMEOUT and TS-RETRY (the bounded read), TS-EXPORT, TS-BUNTEST, TS-TESTFILE.
- `.orly/standards/tests.md`: TEST-NEG, TEST-PUBLIC, TEST-INJECT (the timeout seam), TCF, TEST-ABSENT (hidden names).
- Effect discipline (Indy, Nov 12, 2026: "use a robust code, with better error handling functional programmig approach via effects"): typed `TaggedCliError` failures, `Schema` decoding, no `throw` or `try`/`catch` in new code.
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

Each domain file exports a factory `(visibility) => Command`; none exports a command constant. `tree/index.ts` composes them. `cli.ts` resolves `commandPath` over `buildCommandTree(Unknown)`. Visibility changes only `unlisted` flags, never names, so the path is the same in every tree. It then builds the invocation's tree once (§2) and gives that value to `houseRejection`, `renderLibraryFailure` and `Command.runWith`.

| Current | Destination |
|---|---|
| `list`, `status`, `stop`, `resume`, `kill`, `delete` | `fleet list`, `fleet status`, `fleet stop`, `fleet resume`, `fleet kill`, `fleet delete` |
| `steer`, `logs`, `events` | `fleet steer`, `fleet logs`, `fleet events` |
| `approvals …`, `schedule …`, `memory …` | `fleet approvals …`, `fleet schedule …`, `fleet memory …` |
| `grant list`, `grant delete` | `fleet grants list`, `fleet grants revoke` |
| `models [--provider] [--advanced]` | `models list [--provider] [--advanced]` |
| `tenant provider show`, `create`, `delete` | `models current`, `models use`, `models reset` |
| `secret create/update/show/list/delete`, `workspace secrets` | `secrets create/update/show/list/delete`, `secrets open` |
| `connector list/show` | `connections list/show` |
| `api-key create/list/revoke/delete` | `api-keys create/list/revoke/delete` |
| `billing show` | `usage show` |

`fleet show`, `library` and its children, `chat` and `chat list/rename/delete`, `workspace`, `login`, `logout`, `whoami`, `doctor` and `platform` keep their spelling. `pet` stays runnable and becomes unlisted. Arguments, flags, cardinality, cancellation, ETags, exit codes, redaction and JSON output are unchanged. `chat --fleet` edits setup; `fleet steer` sends a runtime instruction. The guard keys on the root name (`auth-guard.ts:90`); `login` and `pet` remain its only auth-exempt roots.

- **Dimension 1.1** — PENDING — Help and execution agree → Test `canonical_groups_share_help_and_handlers` (unit, `cli/test/command-groups.unit.test.ts`): for every row of `cli/test/fixtures/command-parity.ts` (new spelling, fixture args, expected method and path sequence, recorded once from the old spelling at `4f3159269`), `--help` of the group lists the leaf, and the stubbed fetch records exactly that sequence.
- **Dimension 1.2** — PENDING — Removed spellings have no aliases → Test `removed_root_verbs_are_rejected` (unit, same file): each of the 18 removed root tokens exits 4 with code `UNKNOWN_COMMAND`, and its block's `suggestion` names no removed spelling; `fleet steer F -- --json` sends `--json` as message text.
- **Dimension 1.3** — PENDING — Setup and runtime messages stay distinct → Test `setup_chat_never_calls_runtime_steer` (unit, same file): `chat --fleet F --setup-revision 1 --message x --no-input` records at least one request, all under `/v1/workspaces/{ws}/drafts`, none to `/fleets/F/messages`; `fleet steer F x` records a `POST /v1/workspaces/{ws}/fleets/F/messages` and nothing under `/drafts`.
- **Dimension 1.4** — PENDING — No module-level command constants → Test `tree_modules_export_no_command_constants` (unit, `cli/test/tree-exports.unit.test.ts`): a source scan of `cli/src/program/tree/*.ts` finds no `export const <name> = Command.` and no exported array of commands; a fixture string containing the pattern is detected (positive control).

### §2 — Scope-aware help and completions

**Discovery request.** After `helpArgv`, the invocation is a discovery request when `--completions` appears before `--`, or when the reached path is `[]` or starts with `platform` and either help is requested (empty argv, the `help` verb, `--help` or `-h` before `--`) or the next command token is absent or names no child. `-v`/`--version` are answered before this step and never discover. No other path discovers: `fleet --help` and `whoami` make no extra read.

**Credential eligibility.** Discovery uses the token `resolveToken` would send (environment key first, then `Credentials.getAccessToken`, which rejects malformed stored values). It skips the read when there is no such token, or when `unboundTarget(ctx)` is non-null. It ignores the root exemptions. A stored login is sent only when the normalized target equals the normalized `creds.api_url` that minted it. An environment key is sent only when the target is inferred or comes from `AGENTSFLEET_API_URL`, never to a host named only by `--api`.

**The read.** A discovery layer built from the invocation's `CliConfig` provides only `Credentials` and `HttpClient`. It runs `http.request({ path: USERS_ME_PATH, token, retry: { maxAttempts: 1 }, timeoutMs })`, with `timeoutMs = io.discoveryTimeoutMs ?? DISCOVERY_TIMEOUT_MS` (2000, exported from `discovery.ts`; `RunCliIo.discoveryTimeoutMs` is the test seam). `scopes` is decoded with `Schema.Array(Schema.String)` from the raw body; any failure, timeout, `NetworkError`, `ServerError` or `UnexpectedError` yields `Visibility.Unknown`. A failed or skipped read writes nothing to stdout or stderr. `decodeIdentity`, `whoami` and `login` are untouched. The read runs before the command span and is not traced.

**Visibility.** Each platform leaf declares one `requiredScope`. A leaf is listed when its scope is in the server's list; a family and `platform` are unlisted when none of their leaves is listed. A hidden exact command still parses, reaches the guard and is decided by the server; its own help already names its scope (`platform-models.command.ts:10`). Did-you-mean text comes only from the library's `UnknownSubcommand.suggestions`, which omits unlisted names.

| Leaf | `requiredScope` |
|---|---|
| `platform keys list` | `platform-key:read` |
| `platform keys set`, `delete` | `platform-key:admin` |
| `platform models list`, `show` | `model:read` |
| `platform models create`, `update`, `delete` | `model:admin` |
| `platform runners list`, `show`, `events`, `leases` | `runner:read` |
| `platform runners enroll` | `runner:enroll` |
| `platform runners update`, `rotate-token`, `delete`, `cordon`, `drain`, `revoke`, `self-test` | `runner:write` |
| `platform fleet-library list`, `show`, `stage`, `update`, `publish`, `unpublish`, `delete` | `platform-library:write` (row removed in M249_003) |

**Completions.** `GlobalFlag.Completions` joins Help, Version and LogLevel. `agentsfleet --completions <shell>` prints a static script for the invocation's visibility. `docs/AUTH.md` and the flag description say to write the script to a file and regenerate it after login, a target change or a permission change, because sourcing `$(agentsfleet --completions zsh)` in an rc file makes a read at every shell start. Server capability refresh stays the backend's: 60 seconds fresh, up to 15 minutes stale during an identity-provider outage (`afd_identity/src/capability.rs:3`).

- **Dimension 2.1** — PENDING — Help follows the scope matrix → Test `platform_discovery_matches_effective_scope_matrix` (unit, `cli/test/scoped-discovery.unit.test.ts`): stubs send server closures (`["model:admin","model:read"]`, `["runner:write","runner:read"]`, `["runner:enroll"]`, …); root help, `platform --help` and each family's `--help` list exactly the leaves the table grants.
- **Dimension 2.2** — PENDING — Unknown scopes fail closed → Test `discovery_refuses_malformed_expired_and_unavailable_identity` (unit, same file): a body without `scopes`, `["model:read", 7]`, a 401, a never-resolving fetch with `discoveryTimeoutMs: 50`, a network error and a signed-out run each give exit 0 on `--help`, no `platform` in root help, `platfrom` with no `platform` suggestion, stdout and stderr byte-identical to the signed-out run, and `runCli` resolving within one second.
- **Dimension 2.3** — PENDING — Discovery belongs to its invocation → Test `discovery_does_not_reuse_another_identity_or_target` (unit, same file): env key plus stored login → the read carries the env key; two sequential runs with different credentials see different trees; `--api http://a` then `--api http://b` with a stored login from `http://a` → one read to `a`, none to `b`; an unbound stored login sends no read for `--help`, `login --help`, `logout --help`, `doctor --help` and `pet --help`; `fleet list` and `fleet --help` make zero identity reads; one discovery request makes exactly one.
- **Dimension 2.4** — PENDING — Hidden commands keep backend refusal → Test `hidden_command_and_stale_completion_cannot_bypass_authorization` (unit, same file): with `[]`, `platform keys list` parses, sends `GET /v1/admin/platform-keys` and renders the stubbed 403's `UZ-AUTH-022` in the `code` line with exit 1.
- **Dimension 2.5** — PENDING — Every discovery surface agrees → Test `scoped_help_suggestions_and_four_shell_completions_agree` (unit, `cli/test/completions.unit.test.ts`): for `["model:read"]`, root help lists `platform`, `platfrom` suggests `platform`, and the bash, zsh, fish and powershell scripts contain the word-bounded tokens `platform`, `models`, `list`, `show` and none of `keys`, `runners`, `fleet-library`; for `[]`, no script contains the word `platform`.
- **Dimension 2.6** — PENDING — No credential in discovery output → Test `discovery_writes_no_credential_or_identity_to_diagnostics` (unit, `cli/test/scoped-discovery.unit.test.ts`): `--help --log-level debug` with env key `agt_t_SENTINEL` and identity email `probe@example.test` leaves neither string on stdout or stderr.
- **Dimension 2.7** — PENDING — Scoped help through the built binary → Test `built_binary_help_hides_platform_without_scope` (e2e, deterministic acceptance lane, `cli/test/acceptance/help-and-errors.spec.ts`): the built `dist/bin/agentsfleet.js --help` against the lane's stub server with `[]` lacks `platform`, and with `["model:read"]` lists it.

### §3 — Aligned error presentation

`Command.runWith` runs with `renderErrors: true` and `CliOutput.layerPresenter(presenter)`. The presenter prints a `Help` event's document unchanged, and renders an `InvalidInvocation` event as an error block with no help document. Every failure writer (`renderCliError`, `renderLibraryFailure`, guard refusals through `io.ts` `writeError`, and the unreadable-state warning) uses `output/error-block.ts`.

A block's first line is `✕ error: <message>`. Labelled lines follow with a two-space indent and labels padded to the longest label plus two spaces, so values start at column 17: `code` (the server's `UZ-` code, otherwise the local code such as `UNKNOWN_COMMAND` or `AUTH_REQUIRED`), `suggestion`, `usage` (usage errors), `did you mean` (unknown commands, from `UnknownSubcommand.suggestions`) and `request` (a request identifier). Width is the stream's `columns`, otherwise 80; long values wrap to column 17 using `wrapAt` and `visibleLength`. Colour follows `detectColorMode` (`NO_COLOR`, `FORCE_COLOR`, non-TTY). `--json` envelopes are unchanged. New failure types subclass `TaggedCliError` and join `CliError` and `EXIT_CODE`.

- **Dimension 3.1** — PENDING — One aligned block for every failure → Test `error_blocks_align_labels_and_wrap` (unit, `cli/test/error-blocks.unit.test.ts`): a server error with code, suggestion and request id, a network error, a guard refusal, a usage error (`fleet kill abc`), an unknown command (`lsit`) and `--completions tcsh` each render with every label value starting at column 17 at widths 60 and 120; stdout is empty and stderr contains no `DESCRIPTION` heading.
- **Dimension 3.2** — PENDING — Machine output is unchanged → Test `json_error_envelope_is_unchanged` (unit, same file): under `--json` the same failures produce the literal envelopes recorded in the test from the base (for example `{"error":{"code":"UNKNOWN_COMMAND","message":…}}`), and stdout stays empty.
- **Dimension 3.3** — PENDING — Requested help is untouched → Test `requested_help_prints_unchanged_document` (unit, same file): `fleet --help` and bare `fleet` print the help document on stdout with exit 0 and no block.

### §4 — Server and interface strings, docs and rebase

After `gh pr view <M249_001 PR> --json state` reads `MERGED`, rebase onto it. Then update the UI hint strings and the dashboard specs that spawn the CLI, alongside the server hints, Slack and lease notices, docs, fixtures and playbooks listed in Files Changed. Extend DOC-09 with the removed root verbs. Bump to `0.77.0`; add the Upgrading entry on the docs branch after merge.

- **Dimension 4.1** — PENDING — Every printed command parses → Test `cli_hint_strings_name_real_commands` (unit, `cli/test/command-groups.unit.test.ts`): every `agentsfleet <words>` literal in `cli/src/**`, and every rendered suggestion in the Dimension 1.1 run, resolves through `resolveCommandPath` on `buildCommandTree(Unknown)` to the full word sequence it names.
- **Dimension 4.2** — PENDING — Removed spellings are lint errors → DOC-09 (`docs-removed-command*.yml`) extended with the 18 removed verbs, rule-test positive `agentsfleet kill x`, negatives `agentsfleet fleet kill x` and `agentsfleet secrets list` → `make lint-all` exit 0.
- **Dimension 4.3** — PENDING — Server hints name current commands → Tests `integration_unanswered_notice` and `notice_tests` (existing, Rust) assert `agentsfleet fleet events` and `agentsfleet fleet resume` → `make test-integration-rustd` and `make test-unit-all` exit 0.
- **Dimension 4.4** — PENDING — Rebased on the UI refresh → `manual`: `gh pr view <M249_001 PR> --json state` reads `MERGED` and `git merge-base --is-ancestor <merge commit> HEAD` exits 0; both outputs go in the PR Session Notes.

### §5 — Help report and verification

- **Dimension 5.1** — PENDING — Every help page is reviewed → `manual` (Indy, Nov 12, 2026: "i need a report of the help, and the subcommands helps so i know you have fixed it"): on the final tree, `--help` for root and every group and leaf under three scope sets (`[]`, `["model:read"]`, and the union of every scope in the §2 table), plus one sample of each §3 block, written by a helper kept outside the repository; the report goes to Indy and its path into the PR Session Notes.
- **Dimension 5.2** — PENDING — Lanes are green on the final tree → rubric R1–S4 at `orly gate pr`.

## Interfaces

```
type Visibility = { _tag: "Unknown" } | { _tag: "Known"; scopes: ReadonlySet<string> }
buildCommandTree(visibility: Visibility): Command   // cli/src/program/tree/index.ts
DISCOVERY_TIMEOUT_MS = 2000; RunCliIo.discoveryTimeoutMs?: number
```

Root: `login`, `logout`, `whoami`, `doctor`, `workspace`, `chat`, `fleet`, `library`, `models`, `secrets`, `connections`, `api-keys`, `usage`, scope-advertised `platform`; unlisted `pet`. Global flags: `--api`, `--json`, `--help`/`-h`, `--version`/`-v`, `--log-level`, `--completions bash|zsh|fish|powershell|sh|pwsh`.

- `fleet`: `list [--workspace] [--starting-after] [--limit]`, `show <fleet_id>`, `status`, `stop <fleet_id>`, `resume <fleet_id>`, `kill <fleet_id>`, `delete <fleet_id>`, `steer <fleet_id> [message]`, `logs [fleet_id] [--fleet] [--limit] [--cursor]`, `events <fleet_id> [--actor] [--since] [--cursor] [--limit]`, `approvals {list [--fleet], show <gate_id>, approve <gate_id>, deny <gate_id>}`, `schedule {…as today}`, `memory {…as today}`, `grants {list [--fleet], revoke <grant_id> [--fleet]}`.
- `models`: `list [--provider] [--advanced]`, `current`, `use [--secret <name>] [--model <ref>]`, `reset`.
- `secrets`: `create <name> […typed provider flags]`, `update <name> […]`, `show <name>`, `list`, `delete <name>`, `open`.
- `connections`: `list [--workspace]`, `show <provider> [--workspace]`. `api-keys`: `create [--name] [--description]`, `list [--sort]`, `revoke <api_key_id>`, `delete <api_key_id>`. `usage`: `show [--limit] [--cursor]`. `workspace`: `create`, `list`, `use`, `show`, `delete`.
- Error block labels: `code`, `suggestion`, `usage`, `did you mean`, `request`; values at column 17.

Discovery adds no endpoint and no permission family. HTTP routes are unchanged.

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Scope read unknown or malformed | Security | Visibility `Unknown`; typed commands still reach backend checks | `discovery_refuses_malformed_expired_and_unavailable_identity` |
| Help request names a foreign `--api` | Security | Stored login sent only to its minting deployment; env key never to a flag-only host | `discovery_does_not_reuse_another_identity_or_target` |
| Unbound stored login on an exempt root | Security | Eligibility ignores root exemptions | `discovery_does_not_reuse_another_identity_or_target` |
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

- Plugin and Archive vocabulary, `fleet delete` composition, Plugin `show`/`revisions` readers, route/wire/permission/SQL/status renames, platform catalogue removal and UI adapters: M249_003.
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
- **Model reviews:** EGM design, implementation-readiness and codebase-accuracy reviews ran on Nov 13, 2026 against the first split revision (design: 2 blockers, 9 majors; readiness: 2 blockers, 12 majors; codebase: 2 blockers, 8 majors). Accepted and applied: credential eligibility independent of root exemptions and bound to the minting deployment; tree-then-visibility order; narrowed discovery requests; `retry: {maxAttempts: 1}` and `timeoutMs` with a test seam; a separate strict scope decode leaving `login` and `whoami` untouched; presenter mechanism and every failure writer; `TaggedCliError` variants; server, UI and doc strings owned here; version bump; e2e Dimension; source-scan invariant. Transferred to M249_003: all vocabulary renames, Plugin readers and `fleet delete` composition, because Indy's selected split put "CLI flags" in PR B.
- **Open blockers:** None to building. M249_001 merging gates §4 and the merge.

> Owner (Indy, Nov 12, 2026): "2, 3 can be one spec pushed in your PR. claude will need a handoff prompt to start the spec and create work tree from your merged PR1 by a separate agent. For 2,3 commit in the PR you do for 1, so upon merge we pull and start on this."

> Owner clarification (Indy, Nov 12, 2026): "well if the cli is discrete, then it could start before your merged UI PR. But i will merge the cli after PR merges. so they go in parallel."

> Owner decisions: "Yes flow 1"; "Permanent stop, retain history, no Restore"; "there is no primary vault"; "i dont need a guarded cutover"; "i am only in development".

> Owner decision (Indy, Nov 12, 2026: 11:55 PM), on shipping the ~1000-file rename with the CLI work: selected "Split into two PRs (my call)" — "PR A: command grouping, scoped discovery, completions and the pet. PR B: Plugin + archived rename across Rust, SQL, OpenAPI, CLI flags, UI adapters and docs, rebased onto A."

> Owner (Indy, Nov 12, 2026): "the `agentsfleet pet` make a better pet -- not just ascii. … do this at the end. add this to your punch lisst. You cannot mimic an exact pet like indy sent but its an inspiration." Render choice: "Inline image protocol". Then: "the pet i would move it to a different scope, if it take more scope and libraries and so on."

> Owner (Indy, Nov 12, 2026): "use a robust code, with better error handling functional programmig approach via effects. make the error appear better and aligned." Recorded as §3 and the Effect-discipline rule.

> Owner (Indy, Nov 12, 2026): "i think after you complete i need a report of the help, and the subcommands helps so i know you have fixed it. and the unit test acceptance fixed too for cli." Recorded as Dimension 5.1 and rubric R4.
