<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order follows the executing agent's read order. Author through orly-spec-new.
- No time or effort estimates, complexity ratings, completion percentages or assigned owners.
- Priority sizes the work; dependencies sequence it.
-->

# M223_001: Worktrees share development datastores and process only their own data

**Prototype:** v2.0.0
**Milestone:** M223
**Workstream:** 001
**Date:** Oct 11, 2026
**Status:** DONE
**Priority:** P2 — development infrastructure consumes one stack per repository
**Categories:** API, DOCS, INFRA
**Batch:** B1 — datastore isolation precedes attaching development processes
**Branch:** feat/m223-shared-dev
**Baseline revision:** 9b3d4dae029f5ee344185884b2a40c7dc2c84d09
**Test Baseline:** unit=10027 integration=924
**Baseline evidence:** playbooks/operations/acceptance/baselines/M223_001-20261012.md
**Depends on:** none
**Provenance:** agent-generated (Codex, Oct 11, 2026), from Indy's approved shared-stack proposal
**Canonical architecture:** `docs/architecture/datastore_scaling.md` §Target design and §Limits

## Overview

**Goal (testable):** Two worktrees start independently compiled daemons against the same development Postgres, Dragonfly, Mailpit and Upstash QStash services; their database writes, queues, subscriptions and runner storage remain isolated, and stopping either leaves the other running.
**Problem:** Compose assigns infrastructure to each worktree, duplicating containers and volumes. Sharing the existing deployment-wide readiness and outbound keys would instead let independently compiled daemons consume each other's work.
**Solution summary:** Derive a stable development identifier from each worktree's absolute path. Use one database per worktree on one Postgres server and prefix datastore keys and channels at the Dragonfly boundary. A repository-scoped launcher reuses healthy infrastructure and starts this worktree's native daemon. Existing Linux runner hosts receive separate URL and storage settings. Destructive integration and cluster-fault suites retain their disposable stacks.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat: share development datastores across isolated worktrees
- **Intent (one sentence):** Run concurrent worktrees without paying for another infrastructure stack or mixing the work they execute.
- **Handshake:** Each native daemon uses its worktree database and a validated Dragonfly prefix, including blocking readers and subscriptions. The launcher owns only that daemon; the repository owns the infrastructure snapshot. ASSUMPTIONS I'M MAKING: shared infrastructure is local development only; tenant data models and production defaults stay intact; runner execution remains on the existing Linux host. Quality ceiling: database isolation and one connection-boundary namespace preserve existing business logic; changing every entity's tenant model would add migration and query risk without improving this development workflow. Surface review: no OpenAPI, product command-line flags, release version or application schema changes; internal developer commands and documentation change; no spec/rules conflict.

## Implementing agent — read these first

1. `docker-compose.yml` and `scripts/dragonfly-cluster.sh` — loopback cluster announcements and disposable test topology.
2. `make/dev.mk` and `make/test-infra.mk` — current startup, shutdown and destructive reset callers.
3. `rustd/crates/afd_dragonfly/src/config.rs` and `rustd/crates/afd_dragonfly/src/client.rs` — injected configuration and command ownership.
4. `rustd/crates/afd_dragonfly/src/hub.rs` and `rustd/crates/afd_dragonfly/src/outbound/reader.rs` — subscriptions and dedicated blocking readers.
5. `docs/architecture/datastore_scaling.md` and `docs/architecture/runner_fleet.md` — key families, isolation limits and the Linux runner requirement.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_dragonfly/Cargo.toml` | EDIT | Reuse the workspace validator for namespace input |
| `rustd/Cargo.lock` | EDIT | Record the validator dependency graph if required |
| `rustd/crates/afd_dragonfly/src/{lib.rs,config.rs,client.rs,dedicated.rs,error.rs,error/samples.rs}` | EDIT | Carry validated namespace with every connection; preserve error classes |
| `rustd/crates/afd_dragonfly/src/{namespace.rs,namespace/tests.rs,session/operations.rs}` | CREATE | Validated namespace policy and proofs; split oversized session operations |
| `rustd/crates/afd_dragonfly/src/client/commands.rs` | DELETE | Recombine bounded execution into the existing client within the file cap |
| `rustd/crates/afd_dragonfly/src/test_util.rs` | EDIT | Map fleet history reads and expose primary-local subscription inspection for isolation proofs |
| `rustd/crates/afd_dragonfly/src/{kv.rs,streams.rs,streams/consume.rs,streams/retain.rs,streams/tail.rs,streams/render.rs,ready.rs,outbound.rs,outbound/reader.rs,session.rs,session/tests.rs,capacity.rs}` | EDIT, CREATE | Scope key operations and scans; split existing oversized inline tests |
| `rustd/crates/afd_dragonfly/src/{hub.rs,hub/channels.rs,hub/dispatch.rs,hub/pump.rs,hub/repair.rs,hub/repair/tests.rs,hub/detached.rs}` | EDIT | Scope subscriptions without leaking physical channel names to callers |
| `rustd/crates/afd_dragonfly/tests/{datastore_suite.rs,integration_namespace.rs,keys_and_config.rs}` | EDIT, CREATE | Prove isolation on a real shared cluster |
| `rustd/crates/afd_dragonfly/tests/integration_namespace/sweep.rs` | CREATE | Marker-scoped write and subscription sweep within the test-file length cap |
| `compose.dev-infra.yml` | CREATE | Infrastructure-only development definition, independent of disposable tests |
| `docker-compose.yml` | EDIT | Correct the header describing the disposable container stack; preserve service configuration |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT | Show the shared unsorted chevrons until a person selects a column; keep the default row order |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | Align model copy buttons with the existing Secrets name-slot tokens |
| `ui/packages/app/tests/api-keys-components.test.ts` | EDIT | Pin initial sort affordances and preserve in-place sorting and mutation behavior |
| `ui/packages/app/components/domain/EventDetailsDialog.tsx` | EDIT | De-emphasize identifiers, label source/type and render response markdown through the existing chat renderer |
| `ui/packages/app/tests/event-details-dialog.test.tsx` | EDIT | Prove simplified metadata, rendered markdown and safe untrusted response handling |
| `ui/packages/design-system/src/design-system/DataTableView.tsx` | EDIT | Align sortable numeric values beneath their labels in every consuming table |
| `scripts/{local-dev.sh,local-dev-test.sh}` | CREATE | One launcher for identity, shared infrastructure and processes; separate regression proofs |
| `scripts/{local-dev-state.sh,local-dev-live-test.sh}` | CREATE | Keep state ownership and retained live proofs cohesive within the file-length cap |
| `make/dev.mk` and `Makefile` | EDIT | Extend existing down for orphan teardown; add the requested status target to inspect all shared stacks |
| `make/{quality,test-unit,test-integration-rustd}.mk` | RESTORE | Remove Cargo wrapper wiring; retain original recipes |
| `scripts/{cargo_local.py,cargo_local_test.py}` | DELETE | Remove the rejected configuration launcher and its dedicated tests |
| `~/Projects/dotfiles/{.zshenv,.zshrc}` | EDIT (owning repository) | Load the existing generated environment for non-interactive zsh commands |
| `make/dry.mk` and `make/bench.mk` | EDIT | Read this worktree's daemon URL |
| `README.md`, `docs/AUTH.md`, `docs/architecture/datastore_scaling.md` and `docs/development.md` | EDIT | Record isolation and developer commands; correct native startup, secret loading and worktree URLs |
| `playbooks/operations/ci_rust_images/001_playbook.md` | EDIT | Remove the obsolete development caller of the release builder |
| `docs/v2/{pending,active,done}/M223_001_P2_API_DOCS_INFRA_WORKTREES_SHARE_DEVELOPMENT_DATASTORES.md` | EDIT | Lifecycle metadata and evidence |
| `playbooks/operations/acceptance/baselines/M223_001-*.md` | CREATE | Comparison-revision verification evidence |
| `docs/v2/pending/M218_001_P2_API_DOCS_SHARED_CRATES_REPLACE_HANDROLLED_RUST.md` and `docs/v2/pending/M222_001_P2_API_DOCS_OUTBOUND_PROXY_DECLARED_NOT_INHERITED.md` | PRESERVE | Carry the existing requested spec changes in the same tree |

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md` — NDC (No Dead Code), OWN (One Owner), NSQ (Named constants and Schema-Qualified SQL), UFS (Unified Form for Symbols), FLL (File and Function Length), TST-NAM (Test Naming), MKP (Make Pipeline safety), ORP (Orphan sweep).
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — validated input, injected environments, cause-preserving errors and feature combinations.
- `.orly/dispatch/write_shell.md` — Bash 3 compatibility, quoted arguments, bounded waits and owned cleanup.
- `.orly/dispatch/name_architecture.md` — namespace decisions land in architecture with the first implementation commit.
- `.orly/docs/DOCUMENTATION_RULES.md` — internal development documentation follows runtime behavior.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File and Function Length | yes | Extract existing inline tests before adding to oversized source files |
| Rust errors | yes | One existing crate error hull and alias; invalid namespace refuses configuration |
| UFS and milestone identifiers | yes | Named operational constants; behavior-based source/test names |
| Architecture consult | yes | Extend the deployment boundary with optional development namespaces |
| Documentation reads | yes | Record the exact triggered sections before editing |
| Schema removal | no | Create development databases; change no application schema or migration |
| Deployment/workflow edits | no | Disposable test Compose and deployed configuration remain intact |

## Prior-Art / Reference Implementations

- `afd_dragonfly::ReadyPrefix` and the datastore harness already isolate test key families; extend the existing boundary to all application operations.
- `afd_core::env::EnvSource` and `DragonflyConfig` resolve configuration without global environment mutation.
- `scripts/test-infra-ports.sh` supplies the worktree-path and stable-port precedent; shared development ports have separate ownership from disposable test ports.
- PostgreSQL database isolation preserves the existing schema-qualified statements and migrations without adding worktree identifiers to entity tables.

## Sections (implementation slices)

### §1 — Every Dragonfly surface carries its development namespace
Prerequisite: none. Agent choice: a validated optional namespace is part of connection configuration, cloned with connections and inherited by dedicated readers and the subscription hub. Empty/unset uses existing key names. Namespace text introduces no braces or glob metacharacters, preserving existing cluster hash tags.
- **Dimension 1.1** — DONE — Invalid, oversized or unsafe namespace input refuses before connecting; unset configuration retains existing names → Test `namespace_config_refuses_unsafe_input`.
- **Dimension 1.2** — DONE — Equal logical keys in different namespaces hold independent values, sessions, counters and scan results → Test `namespaces_isolate_values_sessions_and_scans`.
- **Dimension 1.3** — DONE — Fleet streams, readiness and outbound readers process only their namespace, including acknowledgment and pending recovery → Test `namespaces_isolate_streams_readiness_and_outbound`.
- **Dimension 1.4** — DONE — Subscription delivery and reconnect use scoped physical channels while callers receive logical channel names → Test `namespaces_isolate_subscriptions`.

### §2 — Infrastructure is started once and safely reused
Prerequisite: §1. Agent choice: one project per repository, derived from the Git common-directory path; copy the first caller's infrastructure definition and cluster script into repository-scoped development state. Subsequent callers address that canonical snapshot. A bounded exclusive startup lock prevents two callers provisioning the same services concurrently. Healthy existing services are reused without issuing configuration-replacing up commands.
- **Dimension 2.1** — DONE — Stable identities distinguish worktrees and share the infrastructure project across them → Test `worktree_identity_is_stable_and_distinct`.
- **Dimension 2.2** — DONE — Concurrent startup performs one provisioning operation and both callers attach; a second branch cannot replace the snapshot → Test `concurrent_start_reuses_one_infrastructure_stack`.
- **Dimension 2.3** — DONE — Missing Docker, unhealthy dependencies, conflicting ports or a held lock fail with an actionable error and no application startup → Test `infrastructure_refusal_stops_application_start`.

### §3 — Processes, migrations and shutdown belong to the worktree
Prerequisite: §2. Agent choice: start a native daemon compiled from this worktree to preserve the cluster's loopback routing without another application container. Share the repository's Cargo build cache under a build lock, copying the completed binary to the worktree's process directory before releasing that lock. Load the existing trusted machine environment file without printing it. Override only development datastore addresses, namespace and callback origin. Create the worktree database idempotently and migrate it through this worktree's daemon before serving. Process metadata records the executable and process identity; shutdown verifies both before signaling. Stable ports refuse collisions rather than silently selecting another worktree's listener.
- **Dimension 3.1** — DONE — Repeated startup keeps database state and selects the same worktree URL; another worktree uses its own database → Test `development_start_preserves_each_database`.
- **Dimension 3.2** — DONE — Down signals only an owned daemon, preserves data, and never shuts down shared infrastructure → Test `development_down_preserves_sibling_processes`.
- **Dimension 3.3** — DONE — Runner configuration points to this daemon and a separate Linux-host storage root; token provisioning uses the existing runner registration workflow → Test `runner_environment_is_worktree_scoped`.
- **Dimension 3.4** — DONE — Migration failure, port collision, stale process identity and missing configuration produce a nonzero status without signaling foreign processes → Test `development_refuses_unsafe_process_operations`.

### §4 — Developer flows and destructive test lanes stay coherent
Prerequisite: §3. Agent choice: existing up/down use the shared development launcher. Section 8 adds the requested status caller and replaces maintenance shutdown with owner-aware teardown. Mailpit filtering uses distinct development recipients and never global deletion. QStash callbacks use each daemon's reachable origin. No new product endpoint or tenant model is introduced.
- **Dimension 4.1** — DONE — Two real daemons share the infrastructure, answer their own URLs and retain independent data after either stops → Test `two_worktrees_share_infrastructure_end_to_end`.
- **Dimension 4.2** — DONE — Disposable integration resets use their own project and datastore addresses → Test `disposable_reset_keeps_shared_development` (manual: complete integration while two development daemons and four unchanged shared containers remain present).
- **Dimension 4.3** — DONE — Development, dry and benchmark commands select the worktree URL; documentation names runner registration, mail filtering and explicit infrastructure shutdown → Test `runner_environment_is_worktree_scoped` and manual Make dry-run expansion for `dry-app-rustd` and `_bench-loadgen`.

### §5 — Compiler caching reaches every canonical Rust gate
Prerequisite: §4. Agent choice: the existing dotfiles generator owns build settings; zsh startup exports them to ordinary Cargo, Make and orly invocations. Move the shared environment load from interactive startup to `.zshenv`. Delete the Cargo launcher and restore the original Make recipes. Each worktree retains its own target directory while sharing eligible compiler outputs through sccache. No suite or failure check is skipped.
Indy's instruction: "why do you need cargo_local.py and cargo_local_test.py? If its one of script nuke it". The reduced implementation adds no repository helper or configuration parser.
- **Dimension 5.1** — DONE — Non-interactive zsh loads the generated build settings in both existing worktrees → Test `shell_startup_exports_build_settings` (manual, allowlisted output only).
- **Dimension 5.2** — DONE — Missing environment file is tolerated; explicit command-level overrides work; Cargo failure remains nonzero → Test `shell_startup_and_cargo_failures` (manual, temporary startup fixture and canonical Make).
- **Dimension 5.3** — DONE — Existing worktrees reuse eligible compiled dependencies with separate output; canonical lint and unit lanes run their checks without a helper → Test `compiler_cache_across_existing_worktrees` (manual, cache counters and complete Make logs).

### §6 — Reduce namespace plumbing at the datastore boundary
Prerequisite: §5. Independent GPT-6 Astra High and Claude Fable source reviews select the existing concrete, validated `Namespace` with store-specific key builders. Publish `physical_key` on shared and dedicated connections so raw callers can map explicitly; raw execution retains physical-key semantics. Resolve outbound keys once at construction, centralize fleet/session key construction, recombine client execution, fix the unmapped fleet-history test helper and clarify `from_url` defaults. Keep the cluster driver and subscription routing/recovery intact. No trait, mode enum, command parser or new production module is required.
Implementation manifest for this refinement: `src/{client,config,dedicated,kv,outbound,ready,capacity,test_util,streams}.rs`, `src/client/commands.rs` (delete), `src/outbound/reader.rs`, `src/streams/{consume,retain,tail}.rs`, `src/session/operations.rs`, `src/namespace/tests.rs`, `tests/integration_namespace.rs`, `tests/integration_namespace/sweep.rs`, architecture prose and the Compose header. Namespace production code and hub implementation stay as reviewed. The final crate diff is expected to contain 26 files against the base: remove the client extraction, add the existing test-util module and a sweep test module to honor the test-file cap. Preserve the session extraction because its original file exceeds the cap.
Proofs: datastore-free identity, prefix-boundary and hash-tag tests; existing real-cluster isolation/recovery cases; namespaced fleet-history read; prefix-of-prefix isolation; raw physical-key access; and a marker-scoped sweep of writes and active subscription channels on every primary. Run canonical lint, unit and integration lanes at the final boundary. No transport concurrency, retry or command semantics are changed.
- **Dimension 6.1** — DONE — Independent conceptual reviews select the concrete policy and store helpers with the manifest above → Test `namespace_boundary_design_review` (manual, GPT-6 Astra High and Claude Fable source reviews).
- **Dimension 6.2** — DONE — The selected refactor preserves default behavior and isolated values, sessions, streams, scans and subscriptions → Test `namespace_boundary_regression` (namespace unit cases, marker-scoped sweep and complete canonical integration lane after implementation).

### §7 — Requested table presentation matches existing dashboard patterns
Prerequisite: §6. Keep default API-key creation-time ordering separate from an explicit column selection. Match the existing shared table's neutral chevrons, active direction indicators and keyboard controls. Reuse Secrets' desktop name-slot widths for model copy-button alignment. Keep the raw model identifier as the copied value.
- **Dimension 7.1** — DONE — API Keys starts with paired sort chevrons and header clicks still reorder locally → Test `renders Name and Time as clickable, sortable column headers once keys exist`.
- **Dimension 7.2** — DONE — Model names of different lengths share one desktop copy-button column while narrow rows remain usable → Test `the model cell shows the name and keeps the id on hover`.
- **Dimension 7.3** — DONE — Event inspection keeps technical identifiers in copy controls and diagnostics; source and type have readable labels → Test `keeps technical identifiers out of the heading and labels the request source and type`.
- **Dimension 7.4** — DONE — Event replies render markdown through the existing safe chat renderer without treating the entire answer as a title → Test `renders reply markdown without executing embedded HTML`.
- **Dimension 7.5** — DONE — Numeric table values align beneath sortable labels and row actions retain their alignment → Test `numeric_table_alignment` (manual browser measurement and existing shared table behavior tests).

### §8 — Shared stack state survives checkout removal and has visible ownership

Prerequisite: §7. Indy's call: "move the shared dev stack's state out of .git/, and give it an owner-aware status and teardown". Use `${XDG_STATE_HOME:-$HOME/.local/state}/agentsfleet/dev-$(id -u)-$DEV_REPO_ID`; retain the clone identity and project name, with no old-path fallback. Register each root under `worktrees/<id>/worktree` while holding the same infrastructure lock used for teardown. An existing root is live even without a daemon; a missing root is gone. Inspect every stack from its saved snapshot, even after its clone disappears. Extend `make down DEV_STACK=<COMPOSE_PROJECT>` to remove containers and volumes only when no registered root exists; name live roots on refusal and always use `ports.env`. `make status` is the distinct requested all-stack inspection caller; no existing status target exists. Extract state operations and the existing live test to stay within file caps.

- **Dimension 8.1** — DONE — State lives outside Git metadata with stable clone sharing → Test `test_state_home_is_outside_git_metadata`.
- **Dimension 8.2** — DONE — Teardown names and preserves every live registered root → Test `test_stack_teardown_refuses_live_worktrees`.
- **Dimension 8.3** — DONE — Teardown of gone-only registrations uses the saved environment and removes volumes → Test `test_stack_teardown_removes_gone_worktrees`.
- **Dimension 8.4** — DONE — Status lists live and gone registrations and running/stopped containers across stacks → Test `test_status_reports_all_stack_owners`.
- **Dimension 8.5** — DONE — Symlinked state refuses inspection or mutation → Test `test_symlinked_state_is_refused`.

Browser proof: the actual components and production styles with synthetic data at 1440px and 375px. Tokens/duration/cost label and value edges differ by 0px; three model copy buttons share one position. API-key clicks toggle ascending/descending with zero reads. Reply markdown and footer copies render; the mobile page has no horizontal overflow. Preview source and bundles were deleted after verification.

## Interfaces

### UI table refinement requested for this Pull Request
Indy's instruction: "I need them in this PR". API Keys initially shows the same paired sort chevrons as Platform Fleet library. Explicit clicks still sort immediately, with accessible direction state and no fetch. The initial creation-time order and stable tie ordering remain intact. Model library uses Secrets' fixed desktop name slot so short and long names share one copy-button column; narrow screens can shrink the name. Copy keeps the provider model identifier and its existing accessible label.
Indy's Events screenshots add inspection readability to this scope. Keep relative creation time, failure cues, recorded causes, remediation and diagnostic copying. Remove the identifier chip from the heading while retaining its copy action and diagnostic value. Label source and event type in plain text. Render the bounded response with `FleetMarkdown`; preserve the unformatted diagnostic data. Reserve the shared numeric sort-icon space in sortable numeric cells so label and value edges coincide. Keep dates and text left-aligned, numeric values and actions right-aligned, and the sort icon after its label, per `docs/DESIGN_SYSTEM.md`.

This refinement touches four existing components and two behavior tests. Reuse `DataTable`, `CopyButton` and the `trim` width tokens. No new component, endpoint, configuration, dependency or published workflow is introduced. Verify with existing canonical app tests, lint and the final combined PR gate. Larger table or page restructuring adds scope without helping these two requests.

```text
AGENTSFLEET_DEV_NAMESPACE: optional validated ASCII namespace; unset keeps existing keys.
make up: ensure shared infrastructure, compile/migrate/start this worktree's daemon.
make down: stop this worktree's daemon; keep infrastructure and data.
local-dev.sh status: inspect all stacks and registered roots under the state home.
make down DEV_STACK=<COMPOSE_PROJECT>: refuse live roots; remove orphan containers and volumes.
local-dev.sh runner-env: emit existing runner URL/storage settings, never a token.
RUNNER_STORAGE_HOME and AGENTSFLEET_API_URL: existing runner inputs.
DATABASE_URL_API / DATABASE_URL_MIGRATOR: worktree database on shared Postgres.
API_URL: host IPv4 origin on the worktree port, reachable from native clients and QStash.
```

The generated identifier is stable for an absolute worktree path, changes on relocation and never depends on the checked-out branch. Database identifiers and prefixes are derived from its bounded digest. Native daemon startup retains all existing role/configuration requirements. Existing Linux runner-host execution and credentials remain explicit prerequisites; this work does not provision or modify the host.

## Failure Modes

| Mode | Cause | Handling and negative proof |
|------|-------|-----------------------------|
| Unsafe namespace | Glob, brace, delimiter or oversized input | Refuse configuration; `namespace_config_refuses_unsafe_input` |
| Cross-worktree consumption | Shared logical stream/channel names | Scope all readers and writers; `namespaces_isolate_streams_readiness_and_outbound`, `namespaces_isolate_subscriptions` |
| Startup race | Concurrent first callers | Exclusive bounded lock and canonical snapshot; `concurrent_start_reuses_one_infrastructure_stack` |
| Dependency unavailable | Docker, health or port failure | Fail before daemon startup; `infrastructure_refusal_stops_application_start` |
| Migration refusal | Branch migrations fail | Keep both databases and existing daemons; `migrations_precede_serving_and_refusal_preserves_running_daemons` |
| Foreign process | Reused process identifier or occupied port | Verify process identity and refuse signaling; `development_refuses_unsafe_process_operations` |
| Destructive test | Reset or cluster-fault suite | Separate Compose project and datastores; manual `disposable_reset_keeps_shared_development` |
| Exit during shutdown | Process disappears or becomes a zombie between ownership reads or signaling | Confirm exit before clearing ownership; `exit_during_ownership_read_or_termination_cleans_record` |
| Missing runner registration | No token minted in this worktree's database | Existing runner refuses missing/foreign token; `runner_environment_is_worktree_scoped` |

## Invariants

1. One canonical infrastructure definition per repository — startup lock and persisted snapshot enforce ownership; sibling callers do not reconcile it from their branch.
2. Every application key and subscription is scoped once — connection-owned namespace and logical/physical mapping; tests address identical logical keys in separate namespaces.
3. SQL statements and migration catalogs remain unchanged — each daemon connects to its own named database.
4. Worktree shutdown cannot remove shared containers, volumes or sibling processes — separate commands and verified process metadata enforce this.
5. Destructive cluster tests remain isolated — development project selection never becomes the test lane's default.
6. Credentials never enter logs or generated runner settings — settings contain addresses/storage only; existing environment and registration supply secrets.

## Metrics & Observability

Namespace refinement evidence (Oct 12, 2026): `make test-unit-rustd-daemon-libs _RUSTD_UNIT_DAEMON_LIBS=afd_dragonfly` passed with 143 passed, 0 failed and 43 ignored. The final assertions include identity, foreign-prefix rejection and actual cluster hash-slot preservation.
The old fleet-history mapping was restored temporarily for a regression proof. `make test-integration-rustd` failed at the new history assertion: 0 entries instead of 1. With the fix restored, the complete lane passed: 947 passed, 0 failed, including 3 exclusive cluster-recovery tests. Private logs and checksums are in `dragonfly-history-red.json` and `dragonfly-integration-green.json` under the evidence directory below.
`WITH_PROGRESS_DISABLE=1 make lint-rustd` passed Clippy, formatting and both no-feature binary checks. This disables the optional progress display and keeps every check. The final source review and native adversarial pass found no remaining defects. Repository-wide boundary results are recorded in the Pull Request's final session notes after `orly gate pr`.
Compiler-cache evidence (Oct 12, 2026): `/private/tmp/agentsfleet-m223-cache-evidence/` retains private logs and JSON timings. Shell startup checks passed for `zsh -c` and `zsh -lc` in both existing worktrees, with the six cache variables initially removed. Both inherited the same cache directory with no shared target override. Missing-file and command-level override checks passed; a deliberately missing compiler wrapper made canonical daemon-unit Make fail with exit 2.
The final helper-free measurement checkout ran `make test-unit-rustd-runner`: 1228 passed, 0 failed, 111.385 seconds. Active-checkout `make lint-rustd` passed formatting, Clippy and both no-feature binary checks. `make test-unit-rustd` passed: 4528 passed, 0 failed, 948 ignored; 1373.280 seconds including rebuilds and contention. This run verifies correctness, not an isolated speedup. Initial sandbox-restricted attempts failed on process/cache permissions and do not count as successful verification.
Earlier samples with the removed launcher: `make lint-rustd` passed at 138.828 seconds initially and 17.759 seconds warm. `make test-unit-rustd` passed twice with 4528 passed and 948 ignored (302.202 and 215.142 seconds). A fresh separate measurement target recorded 288 Rust cache hits and 77 misses. These single samples ran under shared machine load; they establish reuse, not an isolated speedup benchmark. Launcher-only tests were deleted with their implementation.
Native incremental lint, with `RUSTC_WRAPPER=` and `CARGO_INCREMENTAL=1`, passed at 97.004 seconds initially and 10.719 seconds warm. These samples do not prove a universal winner: incremental compilation serves repeated edits; `sccache` shares eligible dependencies across disposable worktrees. Installed `sccache` 0.18.0 refuses incremental compilation, so the existing machine policy stays at `CARGO_INCREMENTAL=0`.
The generator already emits the six settings. Dotfiles commit `b2ac777` moves the shared environment load to `.zshenv`; existing dotfiles Pull Request #41 contains that startup fix and its 10G cache setting. Protected `master` requires review, so neither repository's pull request was merged. No generated environment file was rewritten.
No product analytics or funnel signals change. The development launcher reports its namespace, daemon URL, dependency failure and attach/start outcome without credentials. Existing daemon telemetry keeps its current meaning.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `namespace_config_refuses_unsafe_input` | Unset/blank → existing names; unsafe/overbound text → configuration error |
| 1.2 | integration | `namespaces_isolate_values_sessions_and_scans` | Equal logical keys in A/B → separate values; scans and deletion see only A |
| 1.3 | integration | `namespaces_isolate_streams_readiness_and_outbound` | Work enqueued in A → B sees none; dedicated A reader recovers/acknowledges A |
| 1.4 | integration | `namespaces_isolate_subscriptions` | A publication → only A receives logical channel; resubscription stays scoped |
| 2.1 | unit | `worktree_identity_is_stable_and_distinct` | Same path → same namespace/port; distinct paths → distinct identity/common project |
| 2.2 | unit | `concurrent_start_reuses_one_infrastructure_stack` | Concurrent stubbed Docker callers → one provision and canonical config reuse |
| 2.3 | unit | `infrastructure_refusal_stops_application_start` | Unhealthy/missing service, held lock or port → nonzero and no daemon invocation |
| 3.1 | integration | `development_start_preserves_each_database` | Repeated A startup → A's row survives; B's database differs |
| 3.2 | unit | `development_down_preserves_sibling_processes` | A down → only verified A signaled; no Compose down or data removal |
| 3.3 | unit | `runner_environment_is_worktree_scoped` | A/B settings → different URL/storage; no token output; existing missing-token refusal |
| 3.4 | unit | `development_refuses_unsafe_process_operations` | Foreign identity → refusal; additional launcher migration-order/refusal and exit-race cases prove no replacement and successful normal-exit cleanup |
| 4.1 | e2e | `two_worktrees_share_infrastructure_end_to_end` | Two subprocess daemons → separate healthy URLs/state on one infrastructure project |
| 4.2 | integration, manual | `disposable_reset_keeps_shared_development` | Disposable integration runs on its own project and URLs; two development daemons and shared container identities survive |
| 4.3 | unit, manual | `runner_environment_is_worktree_scoped` | Runner settings and manual `make -n dry-app-rustd` / `make -n _bench-loadgen` expansions select this worktree's URL |
| 5.1 | manual | `shell_startup_exports_build_settings` | Non-interactive shells in both checkouts export the six generated settings; Cargo targets differ |
| 5.2 | manual | `shell_startup_and_cargo_failures` | Missing file starts normally; command-level override reaches Cargo; failing Cargo makes canonical Make fail |
| 5.3 | manual | `compiler_cache_across_existing_worktrees` | Two existing checkouts, distinct target directories, shared cache hits; full canonical lint/unit results recorded |
| 6.1 | manual | `namespace_boundary_design_review` | Source-based comparison → selected mechanism, exact file manifest and failure proofs before code edits |
| 6.2 | unit, integration | `namespace_boundary_regression` | Default configuration preserves behavior; equal logical names across namespaces remain isolated through every supported surface |
| 7.1 | unit | `renders Name and Time as clickable, sortable column headers once keys exist` | Initial headers show neutral chevrons; existing click cases change direction and row order without another read |
| 7.2 | unit, manual | `the model cell shows the name and keeps the id on hover` | Model name slots match Secrets at desktop; full identifier remains accessible and copied |
| 7.3 | unit | `keeps technical identifiers out of the heading and labels the request source and type` | Identifier chip absent; source/type labeled; identifier copy and diagnostics preserved |
| 7.4 | unit | `renders reply markdown without executing embedded HTML` | Bold text, lists and code render; untrusted HTML creates no executable element |
| 7.5 | unit, manual | `numeric_table_alignment` | Browser label/value edges match; existing sorting, accessibility and row-action checks remain green |
| 8.1 | unit | `test_state_home_is_outside_git_metadata` | XDG state home → shared state outside metadata; same clone still shares one project |
| 8.2 | unit | `test_stack_teardown_refuses_live_worktrees` | Existing registered root → nonzero, named root, no Docker down |
| 8.3 | unit | `test_stack_teardown_removes_gone_worktrees` | Missing registered roots → down with saved ports.env and --volumes |
| 8.4 | unit | `test_status_reports_all_stack_owners` | Two stacks → both projects, live/gone roots and container states reported |
| 8.5 | unit | `test_symlinked_state_is_refused` | Symlinked state → nonzero before Docker or outside-file mutation |

## Acceptance Rubric (single scoring surface)

For R3, replace `<SIBLING_CHECKOUT>` with an existing sibling checkout's absolute path; its `scripts/local-dev.sh` and `scripts/local-dev-state.sh` must match this checkout.

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Namespaced datastore behavior | `make test-integration-rustd` | exit 0; namespace isolation cases pass | P0 | ✅ exit 0; 947 integration passes, including physical-state sweep and raw access |
| R2 | Launcher safety and reuse | `bash scripts/local-dev-test.sh` | exit 0; all launcher cases pass | P0 | ✅ 25 tests; exit 0, ownership/status/teardown and both shutdown exit seams covered |
| R3 | Two-worktree developer flow | `bash scripts/local-dev-test.sh --live "<SIBLING_CHECKOUT>"` | exit 0; both daemon URLs healthy, one infrastructure project, sibling survives stop | P0 | ✅ prior live proof: both URLs, callbacks, data preservation and cleanup; state-home move covered by R2 |
| R4 | Conform | `make harness-verify` | exit 0 | P0 | ✅ ALL GATES GREEN; exit 0 |
| R5 | Lint | `make lint-all` | exit 0 | P0 | ✅ merged-branch invocation exit 0; 158.157 seconds |
| R6 | Unit | `make test-unit-all` | exit 0 | P0 | ✅ 10219 passes, 0 failures, 966 ignored/skipped; exit 0; 561.832 seconds |
| R7 | Version | `make check-version` | exit 0 | P0 | ✅ all versions match 0.59.0 |
| R8 | Secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found; 10 branch commits scanned |
| R9 | Scoped diff | `git diff --name-only origin/main...HEAD` | 0 paths outside Files Changed | P0 | ✅ implementation and carried spec paths listed above |

**Grading protocol:** Record decisive output for spec-specific proofs; repository-wide results come from the final `orly gate pr` run. Baseline counts are measured for the recorded comparison revision before the Pull Request.
R1/R3 retain their recorded live proofs; R5/R6 were rerun after merging `978e696d1`. The final close commit receives the complete PR gate; its current-revision output belongs in the final session notes.

## Dead Code Sweep

Delete the rejected Cargo launcher and its dedicated tests; restore all Make callers. No Rust public symbols are deleted. Replace the existing destructive development down recipe and its help text; keep disposable test-reset consumers unchanged. Any extracted inline tests move with their registration and retain their behavior-based names.

## Out of Scope

- Production deployment configuration, backend changes, tenant columns or SQL query rewrites.
- Sharing destructive reset, failover, migration-fault or saturation suites with development data.
- Provisioning another Linux runner host, remote-host automation or changing sandbox privileges.
- Independent Mailpit inboxes or another scheduling service.
- Sharing running daemon binaries or runner storage across worktrees.

## Product Clarity (authoring record)

1. **Successful user moment** — Start another worktree and find one infrastructure stack with an independently compiled daemon and separate runner settings.
2. **Preserved user behaviour** — Existing daemon APIs, tenant behavior, runner registration and destructive test lanes retain their semantics.
3. **Optimal-way check** — One server per dependency removes container duplication; separate databases and prefixes preserve independence with existing business logic.
4. **Rebuild-vs-iterate** — Iterate at the development launcher and datastore boundary; no new tenancy framework is needed.
5. **What we build** — Namespace support, shared infrastructure definition, worktree launcher, scoped runner settings and behavior proofs.
6. **What we do NOT build** — Remote runner provisioning or shared fault-testing infrastructure; neither is required for the developer moment.
7. **Fit with existing features** — Compose health checks, the real cluster, role-separated migrations and runner configuration remain the underlying mechanisms.
8. **Surface order** — Make commands first; requested table and inspection corrections preserve the dashboard's existing workflows.
9. **Dashboard restraint** — Reuse the shared table, dialog, markdown renderer and copy controls; add no new dashboard abstraction.
10. **Confused-user next step** — Read the launcher error and `docs/development.md`; runner setup uses its existing registration workflow.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Establish namespace correctness first, infrastructure reuse second, process ownership third, then exercise the complete workflow.
- **Alternatives considered:** A common Compose project alone mixes global readiness/outbound workers. Tenant-scoping every query expands the business model and migration surface. Numbered Dragonfly databases are unavailable in real cluster mode. Native Dragonfly namespaces are not assumed to provide the pinned release's replication and subscription behavior without proof.
- **Patch-vs-refactor verdict:** This is a bounded development change with namespace plumbing inside the existing datastore boundary. Native daemon processes avoid new network translation and additional application images; runners retain the existing Linux host.

## Discovery (consult log)

- **Consults:** Indy approved the proposal: "Yes create a spec and lets start the work". Runner decision: "Use the existing Linux runner host". Source inspection found deployment-wide readiness/outbound keys, loopback cluster announcements, no dotenv loading in the daemon entry point and destructive integration resets. Architecture changes land with their implementation.
- **Developer workflow:** Indy requested "i will just use make up , make down in each of my work trees" and "Ensure thatthe envs that you have build are not redundant and standard and reuse the existing." Existing up/down remain the workflow; the requested status caller inspects all stacks. No duplicate runner URL settings are added.
- **Metrics review:** No product analytics or funnel playbook update is required; contributor startup diagnostics expose no secrets.
- **Skill-chain outcomes:** Structural validation clean; unit/integration diff ledgers reviewed with explicit scope exceptions for generic load, allocator and random-order proofs. gstack review and native adversarial pass clean on `51b934c82`; optional external review disabled. Full declared gate passed; the closed metadata receives a fresh gate before publication. Live revalidation passed using the existing sibling checkout, with synthetic state removed and owned cleanup required to succeed. Two source-text tests were replaced by shutdown behavior; Make expansion and measured disposable-stack isolation supply their manual proofs.
- **Deferrals:** None. Existing Linux-host execution is an explicit user decision; provisioning that host is outside this workstream.
Oct 12, 2026: Indy requested compiler-cache wiring and measured lint/unit gate reuse in this existing tree and Pull Request. Indy then required the original Dragonfly refactor in this same workstream and requested GPT-6 Astra High conceptual review. Compose reuse remains under review; disposable test isolation must survive any change.
