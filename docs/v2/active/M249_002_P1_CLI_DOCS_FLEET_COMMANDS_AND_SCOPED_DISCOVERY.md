# M249_002: Fleet commands and scoped discovery

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 002
**Date:** Nov 12, 2026
**Status:** IN_PROGRESS
**Priority:** P1, make the approved command vocabulary and operator capabilities discoverable
**Categories:** CLI, DOCS
**Batch:** B2, independent command work runs alongside the UI refresh
**Depends on:** M249_001 merges before this workstream merges; it does not block starting
**Provenance:** Agent-generated from Indy's approved Flow 1, CLI grouping and permission-discovery decisions; split from the authored M249_002 on Nov 12, 2026 (see Discovery)
**Canonical architecture:** `docs/architecture/api_resources.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** Help, suggestions and completions advertise each fleet command under its resource group and only the platform operations the caller's effective scopes allow, while every typed command keeps its handler and backend authorization.

**Problem:** Fleet verbs are scattered across root help (`cli/src/program/tree/root.command.ts:130`). The library terminology differs from the approved Plugins interface. Root help advertises `platform` and all four operator families to every account.

**Solution summary:** Domain factories build a fresh command tree per invocation. `index.ts` composes it, and a discovery read marks unadvertised platform children `Command.unlisted`. `GlobalFlag.Completions` generates scripts from the same tree. The CLI's Plugin vocabulary (`plugins`, `--plugin`, `--plugin-etag`) lands here against today's API paths; the API, wire, schema and status renames are M249_003.

**Design flow:**

```
argv ─▶ argv-scan (--json, --api, --)      ─▶ loadState + env key   ─▶ guard ctx
                                                                          │
        is discovery request? ──no──▶ tree(Visibility.UNKNOWN) ───────────┤
              │yes, credential, guard passes                              │
              ▼                                                           ▼
        GET /v1/users/me (2 s, once) ─▶ strict scopes ─▶ tree(visibility) ─▶ Command.runWith
                                                                          │
                                         existing handler ◀── existing guard + server scope check
```

**Delivery order:** Start from a fresh worktree at current `origin/main`. M249_001 (UI refresh) merges first, then this branch rebases onto it and passes verification. M249_003 rebases onto this one.

## PR Intent & comprehension handshake

- **PR title (eventual):** Group fleet commands and filter platform discovery by scope
- **Intent:** Make the approved command vocabulary usable, and hide operator commands an account cannot use, without weakening any permission check.
- **Authoring handshake:** The authored spec combined the CLI/API cleanup and scope-aware discovery. Indy split it on Nov 12, 2026: the CLI half is this spec, and the API, wire, schema and caller rename is M249_003.
- **Implementer handshake (Nov 12, 2026: 11:50 PM):**
  - Fetched base `4f3159269` (`git rev-parse origin/main`).
  - UI PR status: `feat/m249-fleet-interface-refresh` has no remote ref (`git ls-remote origin 'refs/heads/feat/m249*'` → empty) and no PR (`gh pr list --search m249` → `[]`).
  - Baseline: `make test-unit-cli` at the base → exit 0, `2223 pass`, `0 fail`, `Ran 2240 tests across 234 files`.
  - Chosen invocation lifecycle: the design flow above. Discovery runs only for a discovery request: empty argv, the `help` verb, `--help`/`-h`, `--completions`, or a command path that does not resolve to a runnable command. Executing commands never pay for the read.
  - Pinned library: `effect@4.1.3`. `Command.unlisted` omits a child from parent help, completion scripts and did-you-mean while keeping exact-name parsing (`effect/dist/cli/Command.d.ts:2000`). `GlobalFlag.Completions` takes `bash | zsh | fish | powershell` (`Completions.d.ts:18`).
  - Scope semantics: `/v1/users/me` renders the stored `ScopeSet`, which is already the downward closure (`afd_api_tenant/src/handler/tenant/identity.rs:93`; `afd_auth/src/scope.rs` `HIERARCHY`). No wildcard scope exists. Visibility is membership in that list.

## Implementing agent — read these first

1. `cli/src/cli.ts`, `cli/src/program/entry/argv-scan.ts` and `cli/src/program/tree/root.command.ts`: credential and target resolution, tree composition and execution order.
2. `cli/src/lib/me-ping.ts`, `cli/src/services/config.ts` and `cli/src/program/auth-guard.ts`: identity decoding, environment-key precedence and the deployment guard.
3. `cli/src/program/tree/platform.command.ts` and its four family files: the independent operator families and the scope each leaf names.
4. `cli/src/program/entry/rejection.ts` and `cli/src/program/tree/resolve-path.ts`: house suggestions and path walking over the same tree.
5. `cli/node_modules/effect/src/cli/Command.ts` (`unlisted`, `withSubcommands`, `runWith`) and `src/cli/GlobalFlag.ts`: the pinned public APIs.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| This spec (`active/` → `done/`) | EDIT/MOVE | Grade the delivered workstream |
| `docs/v2/pending/M249_003_P1_API_UI_DOCS_PLUGIN_AND_ARCHIVED_VOCABULARY.md` | CREATE | The split-out API, wire, schema and caller rename |
| `cli/src/cli.ts` | EDIT | Run discovery, build the invocation tree, enable completions |
| `cli/src/program/tree/root.command.ts` | DELETE | Mixed assembly and leaf-handler module |
| `cli/src/program/tree/index.ts` | CREATE | `buildCommandTree(visibility)` composition; no leaf handlers |
| `cli/src/program/tree/global-flags.ts` | CREATE | `--api` and `--json` definitions only |
| `cli/src/program/tree/auth.command.ts` | CREATE | `login`, `logout`, `whoami` |
| `cli/src/program/tree/diagnostics.command.ts` | CREATE | `doctor`, and `pet` unlisted |
| `cli/src/program/tree/fleet.command.ts` | EDIT | The whole `fleet` group, nesting approvals, grants, schedule and memory |
| `cli/src/program/tree/plugins.command.ts` | CREATE | `plugins` group, moved out of `fleet.command.ts` |
| `cli/src/program/tree/models.command.ts` | CREATE | Catalogue plus the tenant provider choice |
| `cli/src/program/tree/secrets.command.ts` | CREATE | One `secrets` group, moved out of `fleet.command.ts` |
| `cli/src/program/tree/access.command.ts` | EDIT | `api-keys`, `connections`; approvals and grants as fleet children |
| `cli/src/program/tree/workspace.command.ts` | EDIT | `workspace` and `usage`; drop `tenant` and `workspace secrets` |
| `cli/src/program/tree/chat.command.ts` | EDIT | `--plugin`, `--plugin-etag` |
| `cli/src/program/tree/platform*.command.ts` (5 files) | EDIT | Factories taking visibility |
| `cli/src/program/entry/discovery.ts` | CREATE | Discovery-request test, bounded read, scope projection |
| `cli/src/lib/me-ping.ts` | EDIT | Strict scope decoding shared by `whoami` and discovery |
| `cli/src/commands/library/*` → `cli/src/commands/plugins/*` | MOVE/EDIT | Plugin names; add `show` and `revisions` readers |
| `cli/src/commands/chat/*` (flag consumers) | EDIT | Renamed option fields and messages |
| `cli/src/lib/cli-error-render.ts`, `cli/src/program/entry/rejection.ts`, `cli/src/program/entry/deferred-help.ts` | EDIT | One aligned error block; no help dump above a usage error |
| `cli/src/output/error-block.ts` | CREATE | Label-column layout and hanging-indent wrap |
| `cli/test/error-blocks.unit.test.ts` | CREATE | §4 proofs |
| `cli/src/commands/fleet/fleet.ts` | EDIT | `archive` naming; `delete` composes the archive checkpoint |
| `cli/src/commands/workspace/workspace.ts`, `commands/tenant/tenant.ts`, `commands/billing/billing.ts` | EDIT | Moved entry points and user-facing command strings |
| `cli/src/constants/**`, `cli/src/lib/cli-flow.ts` and other help/suggestion strings | EDIT | Next-step hints name current spellings |
| `cli/test/acceptance/**` affected specs and helpers | EDIT | New spellings in the deterministic and live lanes |
| `cli/test/**` affected tests | EDIT | New spellings, negative proofs of removed spellings |
| `cli/test/command-groups.unit.test.ts`, `cli/test/scoped-discovery.unit.test.ts`, `cli/test/completions.unit.test.ts`, `cli/test/fleet-delete-checkpoint.unit.test.ts` | CREATE | Dimension proofs |
| `docs/architecture/user_flow.md`, `docs/architecture/api_resources.md`, `docs/AUTH.md` (CLI examples only) | EDIT | Current command spellings |
| `cli/CHANGELOG.md` or the repository changelog the standard names | EDIT | Removed and renamed commands |
| Separate `~/Projects/docs` branch `chore/m249-cli-commands-changelog` | EDIT | Public CLI reference, after this PR merges |

**Blast inventory (CLI tokens).** Removed root verbs and flags (`list`, `status`, `stop`, `resume`, `kill`, `delete`, `steer`, `logs`, `events`, `approvals`, `schedule`, `memory`, `grant`, `library`, `tenant`, `billing`, `secret`, `connector`, `api-key`, `--library`, `--library-etag`) are found per token with `git grep -n -w -- "agentsfleet <verb>"` and `git grep -n -- "--library"` over `cli/`, `docs/architecture`, `docs/AUTH.md` and `ui/`, excluding `docs/v2/done` and changelog history. Production and test hits are recorded in the PR body at CHORE(close).

## Applicable Rules

- `.orly/AGENTS.md`, `.orly/LOCAL.md`; `.orly/standards/spec.md`, `pr.md`, `tests.md`, `code.md`, `docs.md`, `changelog.md`.
- `.orly/standards/languages/typescript.md`, every rule; `.orly/standards/domains/auth.md` for the discovery read (credential handling, fail-closed).
- Effect discipline (Indy, Nov 12, 2026: "use a robust code, with better error handling functional programmig approach via effects"): typed `Data.TaggedError` failures, `Schema` decoding at the wire boundary, `Effect.timeout`, no `throw` or `try`/`catch` in new code.
- NLR / NLG legacy family: no compatibility aliases (Indy: "i dont need a guarded cutover").

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| `orly gate work`, `orly gate verify`, `orly gate pr` | Yes | Hooks at commit and push; `orly gate pr` by hand at CHORE(close) |
| `make lint-all` | Yes | Includes the CLI TypeScript lint and file/function limits |
| `make test-unit-all` | Yes | Includes `test-unit-cli` and its coverage floor |
| `make test-integration-rustd` | Yes (declared) | No Rust change here; it runs because `orly gate pr` declares it |
| `make check-version` | Yes | No version consumers change |
| `gitleaks detect --redact --no-banner` | Yes | No credential bytes in fixtures |

## Prior-Art / Reference Implementations

- `cli/src/program/tree/*.command.ts`: the existing domain files already declare groups; this work makes them factories rather than module constants.
- `cli/src/program/entry/argv-scan.ts`: the pre-parse scan pattern the discovery-request test extends; it stops at `--` the same way.
- `cli/src/lib/me-ping.ts` `readIdentity`: the identity read discovery reuses, with a stricter scope decoder.
- Indy's reference CLI `oss/js/cli` (Supabase): grouped resource commands with `<group> <verb>` shape.

## Sections (implementation slices)

### §1 — Coherent command composition

Each domain file exports a factory taking the invocation's visibility and returning its group. `tree/index.ts` composes them into the root. `cli.ts` builds the tree once per invocation and hands the same value to `resolveCommandPath`, the house rejection and `Command.runWith`. `root.command.ts` is deleted once help and execution read the new composition.

| Current | Destination |
|---|---|
| `list`, `status`, `stop`, `resume` | `fleet list`, `fleet status`, `fleet stop`, `fleet resume` |
| `kill`, `delete` | `fleet archive`, `fleet delete` |
| `steer`, `logs`, `events` | `fleet steer`, `fleet logs`, `fleet events` |
| `approvals`, `schedule`, `memory` | `fleet approvals`, `fleet schedule`, `fleet memory` (subtrees unchanged) |
| `grant list`, `grant delete` | `fleet grants list`, `fleet grants revoke` |
| `fleet show` | `fleet show` |
| `library`, `library list`, `library delete` | `plugins list`, `plugins show`, `plugins revisions`, `plugins delete` |
| `models`, `tenant provider show/create/delete` | `models list`, `models current`, `models use`, `models reset` |
| `secret *`, `workspace secrets` | `secrets create/update/show/list/delete`, `secrets open` |
| `connector list/show` | `connections list/show` |
| `api-key *` | `api-keys create/list/revoke/delete` |
| `billing show` | `usage show` |

Argument payloads, cardinality, cancellation, ETags, exit codes, redaction and JSON output stay as they are. `chat --fleet` edits setup, and `fleet steer` sends a runtime instruction; setup text is never replayed as a runtime message. `pet` stays runnable and becomes unlisted. The guard keys on the root name (`auth-guard.ts:90`), so `login` and `pet` stay its only auth-exempt roots.

- **Dimension 1.1** — PENDING — Help and execution agree → Test `canonical_groups_share_help_and_handlers` (unit): for every row above, `runCli([...dest, "--help"])` lists the leaf under its group, and invoking it with a stubbed fetch issues the same method and path the old spelling issued at the base.
- **Dimension 1.2** — PENDING — Removed spellings have no aliases → Test `removed_root_verbs_and_library_flags_are_rejected` (unit): `kill <id>`, `library`, `grant list`, `chat --library x` each exit with the usage code and a suggestion naming no removed spelling; `fleet steer <id> -- --json` still sends `--json` as message text.
- **Dimension 1.3** — PENDING — Setup and runtime messages stay distinct → Test `setup_chat_never_calls_runtime_steer` (unit): `chat --fleet <id> --message x --no-input` requests only draft paths; `fleet steer <id> x` requests only the steer admission path.

### §2 — Plugin entry points and fleet deletion

`plugins` keeps Flow 1: a Plugin is a complete fleet Definition with immutable revisions. `plugins show <plugin_id>` and `plugins revisions <plugin_id> [<revision_id>]` read the existing item and revision routes; `plugins list` reads the ordinary collection. `chat --library` becomes `--plugin` and `--library-etag` becomes `--plugin-etag`; `--revision`, `--publish` and `--publish-to` stay. Publication and fleet creation stay separate writes. Paths and payloads are unchanged until M249_003.

`fleet archive` is today's terminal transition. `fleet delete <fleet_id>` on a non-terminal fleet applies that transition, then deletes, with no second confirmation. A confirmed archive is a checkpoint: if deletion then fails, the command reports the fleet archived and names the retry, and a retry skips the archive. An uncertain delete response is reconciled with one read: not found proves deletion, and a denied, unavailable or malformed read never does. There is no restore.

- **Dimension 2.1** — PENDING — Plugin reuse pins a full revision → Test `plugin_revision_creates_independent_private_setup` (unit): `chat --plugin P --revision R --no-input` sends `P` and `R` unchanged in the draft-creation body.
- **Dimension 2.2** — PENDING — Plugin reads use today's routes → Test `plugins_show_and_revisions_read_existing_routes` (unit): `show` and `revisions` request the item and revision paths and render the ETag and review digest; a 404 renders the house not-found shape.
- **Dimension 2.3** — PENDING — Direct deletion keeps its checkpoint → Test `fleet_delete_retains_confirmed_archive_checkpoint` (unit): active → archive then delete; terminal → delete only; archive refused → nothing deleted; delete fails after archive → exit non-zero naming the retry, and the retry sends no archive; uncertain delete then 404 → success; uncertain delete then 403/timeout/malformed → failure.
- **Dimension 2.4** — PENDING — Archive is terminal → Test `archived_fleet_has_history_without_restore` (unit): `fleet resume` on a terminal fleet surfaces the server's refusal; `fleet logs` and `fleet events` still read history; no `restore` verb parses.

### §3 — Scope-aware help and completions

`discovery.ts` decides whether the invocation is a discovery request (design flow). For one, it resolves the credential in play: the environment key outranks the stored login (`cli.ts:99`). It skips the read when there is no credential or when `guardCommand` would refuse the target. Otherwise it makes one `GET /v1/users/me` with a two-second timeout, no retry and no cache. The result lives only in that invocation's tree. It contributes no credential bytes or identity payload to diagnostics.

The scope decoder is strict: `scopes` must be an array of strings only. A missing or non-array value, or any non-string member, makes visibility unknown. Unknown strings grant nothing. `whoami` uses the same decoder; for `whoami` a malformed list renders as an unreadable identity rather than being filtered.

| Held scope (server closure) | Advertised |
|---|---|
| none / unknown | no `platform` in root help, suggestions or completions |
| `platform-key:read` | `platform keys list` |
| `platform-key:admin` | `platform keys list/set/delete` |
| `model:read` | `platform models list/show` |
| `model:admin` | `platform models list/show/create/update/delete` |
| `runner:read` | `platform runners list/show/events/leases` |
| `runner:enroll` | `platform runners enroll` |
| `runner:write` | runner mutations, plus the `runner:read` reads its closure carries |
| `platform-library:write` | `platform fleet-library` children (removed in M249_003) |

A parent is advertised only when a child is. Unadvertised children are `Command.unlisted`, so an exact typed command still parses, reaches the existing guard and is decided by the server; its own help states the scope it needs. A failed read hides advertising and blocks nothing. Unknown discovery is reported nowhere on stdout, so `--json` output stays parseable.

`GlobalFlag.Completions` joins Help, Version and LogLevel. `agentsfleet --completions <shell>` prints a static script for the invocation's visibility. The docs say to regenerate it after login, a target change or a permission change. Server capability refresh stays the backend's: one minute, with up to fifteen minutes of stale authority during an identity-provider outage.

- **Dimension 3.1** — PENDING — Help follows the scope matrix → Test `platform_discovery_matches_effective_scope_matrix` (unit): for each table row, stubbed `/v1/users/me` scopes → `--help`, `platform --help` and each family's `--help` list exactly that row's children.
- **Dimension 3.2** — PENDING — Unknown scopes fail closed for discovery → Test `discovery_refuses_malformed_expired_and_unavailable_identity` (unit): missing `scopes`, `["model:read", 7]`, a 401, a timeout past two seconds, a network error and a signed-out run all produce no `platform` in root help, exit 0 on `--help`, and print nothing extra on stdout under `--json`.
- **Dimension 3.3** — PENDING — Discovery belongs to its invocation → Test `discovery_does_not_reuse_another_identity_or_target` (unit): with both an env key and a stored login, the read carries the env key; two sequential `runCli` calls with different credentials see different trees; a stored login with an inferred unbound target sends no read; one discovery request makes exactly one identity call and an executing command makes none.
- **Dimension 3.4** — PENDING — Hidden commands keep backend refusal → Test `hidden_command_and_stale_completion_cannot_bypass_authorization` (unit): with no scopes, `platform keys list` still parses, sends its request and renders the server's 403 code.
- **Dimension 3.5** — PENDING — Every discovery surface agrees → Test `scoped_help_suggestions_and_four_shell_completions_agree` (unit): for `model:read` only, root help, did-you-mean for `platfrom`, and the bash, zsh, fish and powershell scripts all name `platform models list/show` and no `keys`, `runners` or `fleet-library`.

### §4 — Aligned error presentation

Every text-mode failure renders through one Effect renderer as a block with a fixed label column. The first line holds the `✕` glyph and the message. Labelled lines follow, values starting in one column: `code` (a `UZ-` code when the server sent one), `fix` (the suggestion), `usage` (usage errors only), `did you mean` (unknown commands only) and `request` (a request identifier when present). A value longer than the terminal wraps with a hanging indent to that column. A usage error no longer prints the help document above itself; it prints the block, whose `usage` line names the reached command. `--json` output keeps today's envelope byte for byte. `NO_COLOR` and non-TTY streams get the same layout without colour.

New code in this workstream is written as Effect programs. Failures are `Data.TaggedError` values in the error channel, never thrown and never `try`/`catch`. The discovery read uses `Effect.timeout` and decodes `scopes` with an Effect `Schema`. The delete composition is one `Effect.gen` whose checkpoint is a tagged error the renderer reads.

- **Dimension 4.1** — PENDING — One aligned block for every failure → Test `error_blocks_align_labels_and_wrap` (unit): a server error with code, suggestion and request id, a network error, a usage error and an unknown command each render with every label value starting in the same column at widths 60 and 120; usage errors contain no `DESCRIPTION` heading.
- **Dimension 4.2** — PENDING — Machine output is unchanged → Test `json_error_envelope_is_unchanged` (unit): the same four failures under `--json` produce exactly the envelopes they produce at base `4f3159269`, and stdout stays empty.

### §5 — Documentation, rebase and verification

Update in-repo command examples and the changelog; write the public CLI reference on a `~/Projects/docs` branch after this PR merges. Rebase onto the merged M249_001 before final verification. A check that the UI PR has merged precedes the rebase.

- **Dimension 5.1** — PENDING — One CLI vocabulary → Test `removed_cli_spellings_have_only_historical_references` (unit, source scan): no tracked file outside changelog history and `docs/v2/done` contains a removed root spelling from §1 as an `agentsfleet <verb>` example.
- **Dimension 5.2** — PENDING — Rebased on the UI refresh → `manual`: `gh pr view <M249_001 PR> --json state` reads `MERGED` and `git merge-base --is-ancestor <its merge commit> HEAD` exits 0; both outputs go in the PR Session Notes.
- **Dimension 5.3** — PENDING — Every help page is reviewed → `manual` (Indy, Nov 12, 2026: "i need a report of the help, and the subcommands helps so i know you have fixed it"): on the final tree, capture `agentsfleet --help` and `--help` for every group and leaf under three scope sets (none, `model:read`, every operator scope), plus one sample of each §4 error block. A helper kept outside the repository produces it; the report goes to Indy and its path goes in the PR Session Notes.

## Interfaces

Root: `login`, `logout`, `whoami`, `doctor`, `workspace`, `chat`, `fleet`, `plugins`, `models`, `secrets`, `connections`, `api-keys`, `usage`, plus scope-advertised `platform`; `pet` is unlisted. Global flags: `--api`, `--json`, `--help`, `--version`, `--log-level`, `--completions`.

`fleet`: `list`, `show`, `status`, `stop`, `resume`, `archive`, `delete`, `steer`, `logs`, `events`, `approvals {list, show, approve, deny}`, `schedule {…}`, `memory {…}`, `grants {list, revoke}`. Arguments and flags are the existing ones; `fleet approvals list` keeps its optional `--fleet`.

`plugins`: `list`, `show <plugin_id>`, `revisions <plugin_id> [<revision_id>]`, `delete <plugin_id>`. `models`: `list [--provider] [--advanced]`, `current`, `use --secret <name> [--model <ref>]`, `reset`.

Discovery adds no endpoint and no permission family. HTTP routes are unchanged in this workstream.

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Scope read unknown or malformed | Security | Hide platform advertising; typed commands still reach backend checks | `discovery_refuses_malformed_expired_and_unavailable_identity` |
| Credential or target changes | Security | Visibility lives in one invocation's tree; no cache | `discovery_does_not_reuse_another_identity_or_target` |
| Stored login, unbound inferred target | Security | No discovery read is sent | `discovery_does_not_reuse_another_identity_or_target` |
| Parser and help diverge | Failure | One tree value per invocation feeds both | `canonical_groups_share_help_and_handlers` |
| Delete fails after archive | Failure | Report the checkpoint; retry deletes only | `fleet_delete_retains_confirmed_archive_checkpoint` |
| Uncertain delete response | Failure | One reconciling read; only not-found proves deletion | `fleet_delete_retains_confirmed_archive_checkpoint` |
| A failure renders misaligned or buries the error under help | Failure | One renderer and a fixed label column | `error_blocks_align_labels_and_wrap` |
| Stale completion script | Failure | Docs name regeneration; the server refuses | `hidden_command_and_stale_completion_cannot_bypass_authorization` |

## Invariants

1. Backend authorization decides every operation: discovery only sets `unlisted`, which the parser ignores for exact names (enforced by Dimension 3.4's test).
2. Credential bytes never reach argv, discovery state or diagnostics (enforced by `detectTokenInArgv` and the redaction tests already in `cli/test/cli-flow-secrecy.unit.test.ts`).
3. The tree is a per-invocation value: no domain file exports a command constant (enforced by Dimension 3.3's two-run test).
4. No compatibility aliases: removed spellings fail parsing (enforced by Dimension 1.2's test).

## Metrics & Observability

No signal changed. Command instrumentation keeps its span and analytics row per invocation; `commandPath` now carries the grouped path (`fleet list`), which is the intended label. Discovery adds no event.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | CLI behaviour and discovery (§1–§3) | `make test-unit-cli` | exit 0; Dimension tests pass | P0 | |
| R2 | Package and Rust unit gates | `make test-unit-all` | exit 0; coverage floors held | P0 | |
| R3 | Declared integration lane | `make test-integration-rustd` | exit 0 | P0 | |
| R4 | CLI acceptance, deterministic and live (`cli/test/acceptance/**`) | `make acceptance-local SUITE=cli` | exit 0; `command-matrix-parity` and `help-and-errors` specs updated to the new tree | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Version | `make check-version` | exit 0 | P0 | |
| S3 | Gates | `orly gate work`; `orly gate verify`; `orly gate pr` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

Delete `root.command.ts`, the root-level verb constants, `tenantCommand`, `billingCommand`, `workspace secrets`, and `wsOwnedFleetLibrariesPath` once `plugins list` reads the ordinary collection. Keep historical changelog entries readable.

## Out of Scope

- HTTP route, wire, permission, SQL and status renames, platform catalogue removal and UI adapters: M249_003.
- The `agentsfleet pet` sprite: its own spec after M249_003 (Indy, Nov 12, 2026).
- Flow 2 capability composition, `@plugin` invocation and plugin bindings (M239).
- New platform UI, Plugin ownership or sharing models, a primary vault.

## Product Clarity (authoring record)

1. **Successful user moment** — `agentsfleet fleet --help` lists every fleet verb; an operator holding `model:read` sees `platform models` and nothing else.
2. **Preserved user behaviour** — Handlers, scopes, ETags, recovery and JSON output.
3. **Optimal-way check** — Effect's `unlisted` and `Completions` cover the need; no second parser.
4. **Rebuild-vs-iterate** — Refactor assembly; reuse every handler.
5. **What we build** — Domain factories, Plugin entry points, delete composition, scoped discovery, four-shell completions.
6. **What we do NOT build** — Aliases, a permission cache, a new identity endpoint.
7. **Fit with existing features** — Reuses `/v1/users/me`, the guard and the house rejection.
8. **Surface order** — CLI first; API and UI vocabulary in M249_003.
9. **Dashboard restraint** — No UI change here.
10. **Confused-user next step** — A removed spelling fails with the reached group's `--help`; an exact hidden command's help names its scope.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Five Sections: composition, Plugin entry and delete composition, discovery, error presentation, then docs and rebase.
- **Alternatives considered:** Filtering rendered help text diverges from parsing and completions. A module-global mutable tree carries authority between runs. Discovery on every credentialed invocation adds a round trip to every command and a new failure path to commands that do not need it.
- **Patch-vs-refactor verdict:** Refactor of assembly; handlers stay.

## Discovery (consult log)

- **Consults:** Source inspected at `4f3159269` (see the implementer handshake). The authored spec referenced UI snapshot `3aa7e368b`, which is not present locally (`git cat-file -t 3aa7e368b` → not a valid object).
- **Model reviews:** EGM design and implementation-readiness review pending on this revision.
- **Open blockers:** None to building. M249_001 merging gates the rebase and merge.
- **Deferrals and transfers:** API, wire, schema and UI caller renames to M249_003; the pet sprite to its own spec.

> Owner (Indy, Nov 12, 2026): "2, 3 can be one spec pushed in your PR. claude will need a handoff prompt to start the spec and create work tree from your merged PR1 by a separate agent. For 2,3 commit in the PR you do for 1, so upon merge we pull and start on this."

> Owner clarification (Indy, Nov 12, 2026): "well if the cli is discrete, then it could start before your merged UI PR. But i will merge the cli after PR merges. so they go in parallel."

> Owner decisions: "Yes flow 1"; "Permanent stop, retain history, no Restore"; "there is no primary vault"; "i dont need a guarded cutover"; "i am only in development".

> Owner decision (Indy, Nov 12, 2026: 11:55 PM), on shipping the ~1000-file rename with the CLI work: selected "Split into two PRs (my call)" — PR A command grouping and discovery, PR B the Plugin/archived rename rebased onto A.

> Owner (Indy, Nov 12, 2026): "the `agentsfleet pet` make a better pet -- not just ascii. … do this at the end. add this to your punch lisst. You cannot mimic an exact pet like indy sent but its an inspiration." Render choice: "Inline image protocol" (Kitty/iTerm2 PNG frames, half-block fallback, ASCII for pipes and NO_COLOR). Then: "the pet i would move it to a different scope, if it take more scope and libraries and so on."

> Owner (Indy, Nov 12, 2026): "use a robust code, with better error handling functional programmig approach via effects. make the error appear better and aligned." Recorded as §4 and the Effect-discipline rule.
