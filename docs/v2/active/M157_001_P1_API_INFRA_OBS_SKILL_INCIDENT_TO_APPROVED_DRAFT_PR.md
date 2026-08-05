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

# M157_001: A detected incident ends as one approved draft PR, opened by a fleet that could not have opened it unapproved

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 001
**Date:** Aug 01, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer/operator-facing; the product wedge and the Forge the Future 2026 hackathon entry are the same build
**Categories:** API, INFRA, OBS, SKILL
**Batch:** B1 — single workstream; Sections sequence by dependency
**Branch:** feat/m157-incident-draft-pr
**Test Baseline:** unit=3390 integration=539
**Depends on:** M156_001 (runner leases must complete on the restored dev fleet before any end-to-end proof here can run)
**Provenance:** LLM-drafted (Claude Opus 5, adversarially reviewed by Codex CLI 0.146.0, Aug 01, 2026; redesigned against Indy's restated use case Aug 03, 2026)
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md` (this spec proves its 🔨 rows; its §6 statuses flip here)

---

## Overview

**Goal (testable):** A regression in a customer's instrumented workload is detected by a scheduled sweep over the customer's own Grafana, diagnosed with cited evidence, and — after exactly one human approval that names what it is approving — becomes exactly one draft Pull Request (PR) opened by a repairer fleet that holds no path to open it unapproved.

**Problem:** An operational incident today produces a diagnosis at best. Nothing owns the step from "we know the cause" to "a reviewable fix exists", so code-caused incidents fall into limbo between the on-call person who found the cause and the repository where the fix belongs. The architecture scenario documents this repair path and marks its write half unproven (🔨).

**Solution summary:** Two fleets and the approval gate that already ships. An **investigator** fleet wakes on a cron sweep, reads the customer's Grafana (Elastic second), correlates against repository history, and — when the cause is code-shaped and the repair is a revert of a suspect commit — ends its lease by messaging a **repairer** fleet. The investigator holds a GitHub token minted **read-only**, so it can name a commit and cannot write one. The repairer's incoming event hits the approval gate, which binds *before a lease is issued* (`fleet/approval_gate.zig:1-7`): the event parks, Slack carries an approval naming the proposed action, its evidence, and its blast radius, and the repairer's lease is issued only on approval. The repairer then opens one draft PR through the GitHub HTTP API using a token minted for the declared repositories alone. The human gate is made true by removing `approval_resolve` from the tenant credential grant, so a machine can trigger a repair but only a human can approve one.

**Credentials (milestone gate — enumerate before any M157_002+):**

| Credential | Fetch location | Exists? |
|---|---|---|
| Grafana service-account token | plain workspace secret; demo Grafana provisioned by the §6 playbook | missing |
| Elastic Cloud endpoint + API key | plain workspace secret (second data plane, after Grafana) | missing |
| GitHub App (contents write, PR create, scoped per repository) | platform-configured installation | exists |
| Slack connector | existing workspace connector | exists |
| Model provider key for runs | existing tenant-provider mechanism (vault) | exists |
| Tenant API key for the investigator → repairer hop | dashboard `POST /v1/api-keys` | exists |
| AWS account for EC2 provisioning | 1Password `op://ops` (human-created) | missing |

Grafana, Elastic, and Fly are **plain workspace secrets**, not connectors — there is no `api_key` archetype (`docs/architecture/connectors.md` §Archetypes). Slack, Jira, and GitHub are the connectors. Onboarding is two shapes, not one.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m157): an incident becomes an approved draft PR, and only a human can approve it
- **Intent (one sentence):** An operator whose deploy regressed wakes up to a Slack diagnosis and an approval that says exactly which commit will be reverted in which repository — and clicking it produces one draft PR, where clicking nothing produces nothing.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/AUTH.md` §Scope catalogue + §Provisioning grants + §Flow 3 — the auth dispatch fires on §1; `scopes.zig`'s grant is the file being changed.
2. `src/agentsfleetd/fleet/approval_gate.zig` — the gate checks policy **before a lease is issued** and parks the event; `requestNewGate` is where `ActionDetail` is built and where §2 threads its blank fields.
3. `src/agentsfleetd/fleet_runtime/approval_gate.zig` — `GateDecision`, `GateStatus`, `requestApproval`, and the `.auto_approve` fallthrough at line 96 that makes gating opt-in.
4. `docs/architecture/capabilities.md` §1–§3 — `TRIGGER.md` policy shape, the tool bridge (`${secrets.NAME.FIELD}` substitution, minted GitHub tokens), and the per-lease ExecutionPolicy.
5. `tests/fixtures/fleetbundle/zoho-sprint-daily-summarizer/` — the scheduled-bundle frontmatter shape (`x-agentsfleet: triggers/tools/credentials/network/budget`) the §4 bundles mirror.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/auth/scopes.zig` | EDIT | Split `DefaultGrant`: a machine-credential variant carrying every tenant capability except `approval_resolve`; `.tenant` stays as-is for the human signup claim |
| `src/agentsfleetd/auth/middleware/tenant_api_key.zig` | EDIT | Construct the `agt_t` principal from the machine grant instead of `.tenant` |
| `src/agentsfleetd/auth/scopes_test.zig` | EDIT | The two grants differ in exactly one member; the human claim is unchanged |
| `src/agentsfleetd/http/handlers/approvals/inbox_integration_test.zig` | EDIT | A tenant API key is refused at the resolve route; a user principal is not. Extends the existing suite rather than a new file — it already owns the gate seeding, and the probe needs the api-key middleware wired into the same harness |
| `src/agentsfleetd/fleet/approval_gate.zig` | EDIT | Thread `gate_kind` / `proposed_action` / `evidence` / `blast_radius` into `ActionDetail` from the triggering event; read the recorded gate ref BEFORE any policy read, so a mid-flight `config_json` PATCH cannot withdraw a question already put to a human |
| `src/agentsfleetd/fleet/approval_gate_route.zig` | CREATE | The lookup-vs-policy ORDER as a pure function, so the property is pinned by unit tests rather than by a live Redis and Postgres (RULE FLL split) |
| `src/agentsfleetd/fleet/approval_gate_prose.zig` | CREATE | Making model-authored prose card-safe: C0 controls, DEL, and bidirectional overrides, which otherwise let a claim counterfeit the daemon-derived rows (RULE FLL split) |
| `src/agentsfleetd/http/handlers/fleets/messages.zig` | EDIT | Attribute the steer actor by credential MODE, not by presence of `user_id` — an `agt_t` key carries its creator's id, so machine wakes were recorded as that human |
| `src/agentsfleetd/fleet/approval_gate_integration_test.zig` | CREATE | The parked gate carries a populated detail; the Slack message names the action |
| `src/agentsfleetd/fleet/repairer_gate_integration_test.zig` | CREATE | Dimensions 3.3 + 3.4 driven through the real lease path against the SHIPPED bundle's own config, converted by the install path's own `parseTriggerMarkdownWithJson`. The generic gate lifecycle is already covered against a hand-written config; what was not covered is that the bundle we ship refuses to run without a human |
| `src/agentsfleetd/credentials/integration_github.zig` | EDIT | Mint body carries `repositories` + `permissions` instead of `""` |
| `src/agentsfleetd/fleet/repair_proposal.zig` | DELETE | Superseded — approval binds a bounded run, not bytes; no daemon apply exists to re-validate against |
| `src/agentsfleetd/fleet/repair_proposal_test.zig` | DELETE | Tests of the deleted kernel |
| `src/agentsfleetd/fleet/repair_bounds.zig` | DELETE | Apply-time diff bounds with no apply site |
| `src/agentsfleetd/fleet/repair_bounds_test.zig` | DELETE | Tests of the deleted kernel |
| `src/agentsfleetd/tests.zig` | EDIT | Drop the two deleted module registrations |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | Retire `UZ-REPAIR-001..005` — every one names an apply-service failure that no longer has a site |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Retire the `UZ-REPAIR-*` constants |
| `src/agentsfleetd/errors/gen_error_codes.zig` | EDIT | Retire the REPAIR category copy (its comptime coverage gate pairs with the family) |
| `library/incident-responder/SKILL.md` | EDIT | Becomes the investigator: read-only, no repair authorship, ends by naming a repair intent |
| `library/incident-responder/TRIGGER.md` | EDIT | Keep the `github` credential and `api.github.com` — the investigator reads commit history to correlate, and cannot name a suspect commit without them. Declare `repository_access: read` so its minted token carries no write permission. Add `memory_store` + `memory_recall` to the declared `tools`, which today lists `http_request` alone and so cannot satisfy Dimension 4.9 |
| `library/incident-repairer/SKILL.md` | CREATE | The revert rung: given a suspect commit, produce one draft revert PR; refuse on conflict; nothing else |
| `library/incident-repairer/TRIGGER.md` | CREATE | `api` trigger, `git` + `http_request`, `github` credential, `api.github.com` allowlist, top-level `repositories` binding, **non-empty `gates.rules`** |
| `src/agentsfleetd/fleet_runtime/config_types.zig` | EDIT | Top-level `repositories` egress binding on the fleet config, distinct from the webhook trigger's ingress binding |
| `src/lib/contract/execution_policy.zig` | EDIT | `repositories` + `repository_access` as additive defaulted fields, so the binding the mint scopes by also reaches the runner that must refuse an out-of-binding fetch BEFORE any network call. Absent → empty → fail closed |
| `src/agentsfleetd/fleet/service.zig` | EDIT | `resolveExecutionPolicy` populates the two fields from the same fleet config the mint reads, so the two rings cannot disagree |
| `src/runner/daemon/lease_run.zig` | EDIT | `FetchForwarder` beside `MintForwarder` — forwards the child's on-demand fetch ask, lease-bound server-side |
| `src/runner/child_supervisor.zig` | EDIT | `FetchHook` alongside `MintHook` on the supervisor surface |
| `src/runner/pipe_proto.zig` | EDIT | Two frame types beside the credential pair — `repo_fetch_request` up the child's stdout, `repo_fetch_response` back down its stdin. No new descriptor and no new sandbox hole, exactly as the mint channel added none |
| `src/runner/engine/repo_fetch_request.zig` | CREATE | The child half of the fetch channel, mirroring `credential_request.zig`: the ask names the repository, commit, and branch and nothing else — no workspace, no lease id, no path — so a prompt-injected child can ask for the wrong repository and be refused, but cannot ask for the wrong PLACE. The reply is a workspace-relative path, because an absolute one would tell the sandbox a fact it otherwise never has |
| `src/runner/child_supervisor_read.zig` | EDIT | `FetchHook` + `FetchOutcome` + the `repo_fetch_request` frame arm. Both outcome slices are borrowed static strings, so framing a reply has no allocation and therefore no failure path of its own |
| `src/runner/engine/runtime/repo_fetch.zig` | CREATE | The child-side tool — the only way a fleet obtains a working tree. It names the repository, the commit, and the branch, and cannot name a workspace, a path, or a credential; a refusal comes back as prose the model reformulates against, and a success puts the single word `repo` into context |
| `src/runner/engine/runtime/repo_fetch_tool_test.zig` | CREATE | Driven exactly as NullClaw drives it — arguments in, `ToolResult` out — over real pipes: the ask reaches the wire verbatim, a refusal carries its reason, a run with no channel fails closed, and the declared schema matches the arguments `execute` actually reads |
| `src/runner/engine/tool_builders.zig` | EDIT | `buildRepoFetch`, deriving the fetch channel from the lease's mint channel — one duplex, multiplexed by frame type, so the two cannot drift apart |
| `src/runner/engine/tool_bridge.zig` | EDIT | One `BRIDGE_REGISTRY` row beside `git`, and the tool census that guards against registry drift |
| `src/runner/repo_fetch_channel_test.zig` | CREATE | The parent half over real pipes: the ask reaches the hook verbatim, the path is framed back relative, a null hook and a malformed ask both refuse with a reason, and a lease that asks for nothing invokes the hook zero times |
| `src/runner/child_supervisor_test.zig` | EDIT | The new `readResult` parameter at every existing call site |
| `src/runner/child_supervisor_edge_test.zig` | EDIT | Same |
| `src/runner/child_supervisor_concurrency_test.zig` | EDIT | Same |
| `src/runner/credential_mint_e2e_test.zig` | EDIT | Same |
| `src/runner/daemon/renew_driver_edge_test.zig` | EDIT | Same |
| `src/runner/repo_fetch.zig` | CREATE | The ask's refusal surface: a PURE function of the binding and the ask, so "refused before any network call" is a unit test rather than an observation. Also derives the remote URL from the BINDING's spelling, never the model's |
| `src/runner/repo_fetch_test.zig` | CREATE | Binding refusal, malformed repository / commit / head, the declared-spelling rule |
| `src/runner/RepoFetchTarget.zig` | CREATE | The daemon-owned directory the fetch lands in. File-as-struct because it owns two open handles and the claim they represent: create-exclusive (a squatting child loses the fetch rather than redirecting it), `O_NOFOLLOW` open (a symlink swap after the create meets ELOOP), beneath-only canonical verify. Never opened by name after the claim — every git step runs against the handle |
| `src/runner/repo_fetch_target_test.zig` | CREATE | The three squat kinds refuse; a squatting symlink's target is provably unwritten; a missing workspace is a distinct refusal from a squat |
| `src/runner/repo_fetch_bounds.zig` | CREATE | What actually bounds the fetch: one child run under a shared absolute deadline and a measured byte ceiling. Depth bounds HISTORY, not BYTES, and the daemon-side fetch runs outside the child's cgroup, so the ceiling is measured on a tick and the run killed on breach — **before** the single blocking `wait`, never after it |
| `src/runner/repo_fetch_bounds_test.zig` | CREATE | The measure (nesting, symlinks, ceiling short-circuit, empty tree) and the two kills (deadline, quota), each proven to return promptly rather than wait the child out |
| `src/runner/repo_fetch_exec.zig` | CREATE | The three git steps — `init`, depth-2 `fetch` of the suspect and the head into two named refs, detached `checkout`. Fetching by URL rather than adding a remote is what keeps any credential out of `.git/config` |
| `src/runner/repo_fetch_env.zig` | CREATE | The environment those steps run under, and the one place the minted credential is written: a URL-scoped `http.<url>.extraheader`, never argv (`/proc/PID/cmdline` is world-readable where `environ` is owner-only). Nothing is inherited — the daemon's own environ and the host's `~/.gitconfig` are both excluded, so an operator's credential helper or `insteadOf` rewrite cannot redirect a fleet's fetch. Split from the exec module because it is the security-critical half and earns its own reading and its own tests |
| `src/runner/repo_fetch_exec_test.zig` | CREATE | Against a local `file://` fixture — no network, no credential. Six commits upstream, three fetched; the token absent from every path and byte of the tree; and the repairer's own `git revert` run to completion, which is what proves the suspect's parent came down |
| `src/runner/tests.zig` | EDIT | Register the new runner modules for test discovery |
| `src/runner/daemon/StorageHome.zig` | CREATE | The exclusive claim on the storage home and its startup reaper for orphaned per-lease workspaces — `defer cleanupWorkspace` does not run on SIGKILL / out-of-memory kill / reboot, and a leaked workspace now holds a repository rather than a few hundred KiB. File-as-struct because the claim is state the type owns (an open directory + a process-lifetime lock), which makes "sweep without holding the home" unrepresentable — the Single-Type-Module rule outranks this table's earlier `storage_sweep.zig` name |
| `src/runner/daemon/storage_home_test.zig` | CREATE | A lease-shaped directory is reaped; a dot-prefixed cache directory, a non-lease name, a lease-shaped symlink, and a lease-shaped regular file are not. Plus the three refusals: shallow path, contended lock, un-adopted home |
| `src/runner/main.zig` | EDIT | Call the sweep after the storage-home `mkdir`, before the poll loop |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/AddLibraryDialog.tsx` | EDIT | Offer an upload source beside GitHub; post `source_kind:"upload"` with both markdown bodies |
| `cli/src/program/cli-tree-fleet.ts` | EDIT | Verb creating a library entry from a local bundle directory |
| `cli/src/commands/fleet_library_upload.ts` | CREATE | Reads `SKILL.md` + `TRIGGER.md`, posts `source_kind:"upload"` |
| `src/agentsfleetd/fleet_runtime/crew_bundle_test.zig` | CREATE | Both shipped bundles, asserted as the parser and the gate actually see them — one file rather than the per-bundle `bundle_gate_test.zig` this table first named, because every assertion is a comparison BETWEEN the two halves (read vs write binding, which tools each declares, that neither holds a tenant key). Reads `library/` from disk: each property is one a bundle author can break by editing markdown, and none would fail any other test in the tree |
| `docs/AUTH.md` | EDIT | Record that machine credentials cannot resolve approvals, and why |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | Describe the gate-bound two-fleet design; flip proven rows |
| `bench/incident-response/` | KEEP | §5 landed and is frozen; its rubric rows are rewritten to claim only detection |
| `build.zig` | EDIT | Register the §6 benchmark executable and the new named-module test lanes |
| `make/bench.mk` | EDIT | `make bench-incident` — the §6 rubric row R6 runs through it |
| `src/agentsfleetd/credentials/broker.zig` | EDIT | Fold the repository binding + access level into the mint path; the cache key work moved to `broker_key.zig` (RULE FLL split) |
| `src/agentsfleetd/credentials/broker_key.zig` | CREATE | The cache key, split out and framed. `bindingFingerprint` joined repositories without length framing, so `["acme/a","acme/b"]` and the single entry `"acme/a<SEP>acme/b"` hashed IDENTICALLY — a deterministic alias needing no collision search, on the exact key that decides which fleet's token a second fleet receives |
| `src/agentsfleetd/credentials/broker_test.zig` | EDIT | The framed key, and the two-fleets-one-workspace collision the review found |
| `src/agentsfleetd/credentials/integration.zig` | EDIT | `RepositoryBinding` on the integration surface; the registry table split to `integration_registry_test.zig` |
| `src/agentsfleetd/credentials/integration_ctx.zig` | EDIT | `MintCtx.repository_binding` — what the GitHub mint scopes by |
| `src/agentsfleetd/credentials/integration_github_mint_body_test.zig` | CREATE | Dimension 2.3: `write` mints contents + pull_requests, `read` mints contents alone and carries NO pull_requests key; an unbound fleet mints nothing |
| `src/agentsfleetd/credentials/integration_github_body.zig` | CREATE | What the mint ASKS for — the request-body builder and the bare-name reduction, split out when the reach check pushed `integration_github.zig` past RULE FLL |
| `src/agentsfleetd/credentials/integration_github_reach.zig` | CREATE | Whether the token GitHub RETURNED reaches what was declared. The owner never rides the wire, so only the response can say — this is what stops the `${secrets.github}` path meaning something different from the fetch path |
| `src/agentsfleetd/credentials/integration_registry_test.zig` | CREATE | Registry coverage, split from `integration.zig` to keep it inside the line budget |
| `src/agentsfleetd/credentials/testing.zig` | EDIT | A bound fixture (`test_binding`) so tests whose subject is something else are not all asserting the unbound refusal |
| `src/agentsfleetd/cron/FireQueue.zig` | EDIT | Orphan sweep from the retired repair kernel (RULE ORP) |
| `src/agentsfleetd/errors/error_lookup.zig` | CREATE | Lookup split from the registry when the `UZ-REPAIR-*` family was retired (RULE FLL) |
| `src/agentsfleetd/fleet/approval_gate_detail.zig` | CREATE | Builds the `ActionDetail` §2 threads, including the daemon-vouched `- Token reaches:` line the card previously had no way to state |
| `src/agentsfleetd/fleet/fleet_session.zig` | EDIT | Orphan sweep from the retired repair kernel |
| `src/agentsfleetd/fleet_runtime/approval_gate.zig` | EDIT | The `.auto_approve` fallthrough Dimension 3.2 guards, and the detail fields §2 populates |
| `src/agentsfleetd/fleet_runtime/approval_gate_slack.zig` | EDIT | Render the evidence in a code span after the attributed claim, so model prose cannot counterfeit the daemon-derived rows above it |
| `src/agentsfleetd/fleet_runtime/config_gates.zig` | EDIT | Gate policy parsing beside the new repository binding |
| `src/agentsfleetd/fleet_runtime/config_gates_test.zig` | CREATE | Gate-policy parse coverage, split from the parser suite |
| `src/agentsfleetd/fleet_runtime/config_parser.zig` | EDIT | Parse the top-level `repositories` + `repository_access` binding; the repository half split to its own module (RULE FLL) |
| `src/agentsfleetd/fleet_runtime/config_repositories.zig` | CREATE | The binding parser. The two keys are optional TOGETHER — a list without an access level does not know how far to reach and an access level without a list does not know what to reach, and either would have to fall back to the installation's full scope |
| `src/agentsfleetd/fleet_runtime/webhook_constants.zig` | CREATE | Provider constants shared by the verifier and the ingress handlers (RULE UFS) |
| `src/agentsfleetd/fleet_runtime/webhook_verify.zig` | EDIT | Uses the shared constants; the HMAC-over-body scheme §3 cites for why a signed webhook cannot substitute for the wake |
| `src/agentsfleetd/http/handlers/auth/identity_events_clerk.zig` | EDIT | The second consumer of `grantMembers(.tenant)` — the human signup claim §1 must leave untouched |
| `src/agentsfleetd/http/handlers/connectors/slack/events.zig` | EDIT | Resident-fleet resolution, cited in Decomposition alternative (c) for why the crew handoff does not route through Slack |
| `src/agentsfleetd/http/handlers/connectors/slack/events_integration_test.zig` | EDIT | Follows the handler change |
| `src/agentsfleetd/http/handlers/connectors/slack/thread_refetch_integration_test.zig` | EDIT | Follows the handler change |
| `src/agentsfleetd/http/handlers/ingress/github.zig` | EDIT | Shared webhook constants; ingress `repositories` stays the INGRESS binding and is not overloaded by the egress one |
| `src/agentsfleetd/http/handlers/ingress/github_integration_test.zig` | EDIT | Follows the handler change |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | Thread the fleet's binding into the mint; scope resolution split out (RULE FLL) |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_scope.zig` | CREATE | Resolve the lease's workspace, fleet, and repository binding in one read. A config that fails to parse degrades to NO binding, so a malformed config withholds a token rather than widening one |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_integration_test.zig` | EDIT | The grant-gate suite; its GitHub-minting test now seeds a bound fleet, because §2 made an unbound GitHub mint fail closed |
| `src/agentsfleetd/http/handlers/runner/sql.zig` | EDIT | `SELECT_LEASE_SCOPE_FOR_MINT` returns the fleet's `config_json` so the binding is read on the same query |
| `src/agentsfleetd/http/handlers/webhooks/fleet.zig` | EDIT | Shared webhook constants |
| `src/agentsfleetd/http/handlers/webhooks/github.zig` | EDIT | Shared webhook constants |
| `src/agentsfleetd/http/route_scopes_test.zig` | EDIT | Dimension 1.4 — the resolve route's requirement and the machine grant are provably disjoint |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | Follows the shared-constant extraction |
| `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md` | EDIT | Gains the repository binding, so a fail-closed mint does not stop the fleet installed from it (Indy's call: fail closed anyway, and let M154's teardown take the installed row) |
| `playbooks/demo/forge-2026/` | CREATE | EC2 + collector + Grafana bring-up, failure injection, replay proof |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NDC (no dead code at write time — the `UZ-REPAIR-*` family and the repair kernel go out in this diff, not later), NLR (touch-it-fix-it), NLG (no legacy framing pre-2.0.0 — the scenario doc describes the gate-bound design as *the* design), UFS (gate status strings, bundle names shared verbatim across surfaces), ORP (orphan sweep), FLL (length caps).
- `~/Projects/dotfiles/dispatch/write_auth.md` → **`docs/AUTH.md` before any edit to `auth/scopes.zig`** — §1 is a scope-grant change and carries the AUTH review profile.
- `~/Projects/dotfiles/dispatch/write_zig.md` — pg-drain, tagged-union results, errdefer, cross-compile; all new daemon and runner code.
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — §4a's CLI command and dashboard dialog: TS FILE SHAPE at PLAN, `const`/import discipline, design-system primitives over raw HTML, token utilities over arbitrary values.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` + `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured events, init/deinit lifecycles; `~/Projects/dotfiles/dispatch/write_shell.md` — playbook scripts, and vault-reading playbook scripts pass both vault gates (`check-vault-gate-parity`).
- Schema conventions: **not applicable** — this workstream adds no table and no migration, so no `schema/embed.zig` slot is claimed and no collision with M154's rebuild exists.
- REST guidelines: not applicable — no new public HTTP route; approval rides existing gate surfaces.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; linux test graph clean |
| PUB / Struct-Shape | yes | FILE SHAPE DECISION for the edited `ActionDetail` surface |
| File & Function Length | yes | detail threading stays inside `requestNewGate`'s budget or splits |
| UFS | yes | gate kinds, bundle names, mint permission keys as named constants |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes | structured events; the retired `UZ-REPAIR-*` family leaves the registry coverage gate green |
| SCHEMA GUARD | no | no schema file, no migration array edit |
| UI Substitution / DESIGN TOKEN | **yes** | §4a edits `AddLibraryDialog.tsx` — design-system primitives only, no raw HTML, no `*-[...]` arbitraries |
| TS FILE SHAPE / Bun discipline | **yes** | §4a adds a CLI command and edits the fleet command tree; `dispatch/write_ts_adhere_bun.md` fires |

## Prior-Art / Reference Implementations

- **Reference:** `tests/fixtures/fleetbundle/{platform-ops,github-pr-reviewer,zoho-sprint-daily-summarizer}` — bundle prose style, constraint framing, scheduled frontmatter; the §4 bundles are their production-grade siblings.
- **Reference:** `docs/architecture/connectors.md` §GitHub App + §Bounded outbound — daemon-side token minting and armed vendor calls; the mint narrowing extends this, never re-invents it.
- **Reference:** `src/agentsfleetd/fleet_runtime/approval_gate_async.zig` — the re-poll shape that lets a parked event resume without a blocked thread; §2 adds no parallel machinery.
- **Benchmark:** `bench/incident-response/` — landed in this workstream; measures detection only.

## Sections (implementation slices)

### §1 — Machines trigger, humans approve

`grantMembers(.tenant)` closes `approval_resolve` into every `agt_t` key at comptime (`auth/scopes.zig:99-110`), and `core.api_keys` carries no per-key scope column. Any fleet holding a tenant key — including the investigator, which needs one to message the repairer — can therefore resolve the very gate that guards the repairer. The gate is not a human gate until that is false.

**The grant is shared, so it must be split rather than trimmed.** `grantMembers(.tenant)` feeds two consumers: `defaultScopes(.tenant)` at `auth/middleware/tenant_api_key.zig:119` (machine `agt_t` principals) **and** `defaultClaim(.tenant)` at `http/handlers/auth/identity_events_clerk.zig:41`, which becomes `DEFAULT_SIGNUP_SCOPES` written to Clerk `publicMetadata` at signup and read back as the human tenant owner's `scopes` claim. Deleting the scope from the shared list would strip approval rights from every new tenant owner. `DefaultGrant` therefore gains a third variant for the machine credential, carrying every tenant capability except `approval_resolve`; the human signup claim keeps `.tenant` unchanged.

The two consumers also differ in when the change bites, and the spec relies on both behaviours. `defaultScopes` is evaluated at principal construction on every request, so **existing `agt_t` keys lose the scope immediately with no backfill**. `defaultClaim` is persisted per-user at signup, so existing owners keep what was already written and only new signups read the current constant — which is why the human list must not change at all.

- **Dimension 1.1** — **DONE** — The machine grant carries every tenant capability except `approval_resolve` → Test `test_machine_grant_excludes_approval_resolve`
- **Dimension 1.2** — **DONE** — The human signup claim still carries `approval:resolve`; the two grants differ in exactly that one granted member → Test `test_signup_claim_retains_approval_resolve`
- **Dimension 1.3** — **DONE** — A tenant API key is refused at the approval-resolve route; a user principal still passes → Test `test_api_key_cannot_resolve_approval`
- **Dimension 1.4** — **DONE** — The resolve route's requirement and the machine grant are provably disjoint, so no caller can resolve with a machine credential regardless of who calls → Test `test_no_machine_approval_callers`

### §2 — The approval names what it is approving, and the token reaches one repository

`requestNewGate` builds `ActionDetail` with `gate_kind`, `proposed_action`, `evidence`, and `blast_radius` left blank — the code comment records them as designed-but-unthreaded. A human approving a repair sees none of it. This Section threads them from the triggering event so the Slack approval states the repository, the suspect commit, the evidence that implicated it, and that the outcome is one draft PR. Separately, `integration_github.zig` mints installation tokens with `.body = ""`, which yields the App's full permissions across **every** repository in the installation; the mint body carries `repositories` and `permissions` derived from the fleet's declared binding instead.

- **Dimension 2.1** — **DONE** — A parked gate carries a populated `proposed_action`, `evidence`, and `blast_radius` → Test `test_gate_detail_is_populated`
- **Dimension 2.2** — **DONE** — The Slack approval message names repository, commit, and outcome → Test `test_slack_approval_names_the_action`
- **Dimension 2.3** — **DONE** — The mint request body pins `repositories` to the fleet's binding and `permissions` to the level its `repository_access` declares — read mints `contents: read` alone, write mints contents + pull-requests → Test `test_mint_body_is_repository_and_access_scoped`
- **Dimension 2.4** — **DONE** — A fleet with no declared repository binding, or none declaring `repository_access`, gets no mintable GitHub token (fail closed) → Test `test_unbound_fleet_mints_nothing`

### §3 — The write lives behind the gate, structurally

The investigator reaches `api.github.com` with a token minted **read-only**, so its inability to open a PR is a property of the credential the daemon hands it rather than of its prompt. Removing its GitHub access altogether was the earlier design and it does not work: `incident-responder/SKILL.md:53-59,89,115` reads `GET /repos/{owner}/{repo}/commits`, correlates deploy annotations against commit history, and verifies `base_sha` against the branch head — a fleet with no GitHub reach cannot name the suspect commit that Dimension 4.2 requires and the repairer's message depends on. Read is the job; write is the boundary, and §2.3's mint narrowing is where the boundary lives. The repairer's bundle declares a non-empty `gates.rules` — because `approval_gate.zig:96` falls through to `.auto_approve` when nothing matches, an omitted rule silently yields an autonomous agent holding a write token. Approval authorises **one bounded repairer run**, not specific bytes; the draft PR is the review surface where the diff is read.

**The investigator does not wake the repairer in this workstream, and that is a security decision rather than a scoping one.** `route_scopes.zig` maps `.workspace_fleet_messages` and the PATCH arm of `.patch_workspace_fleet` to the *same* scope, `fleet:write`. `patch.zig` accepts `trigger_markdown` or `config_json`, and `config_json` is where `gates` lives. So any credential able to send the wake message is also able to rewrite the repairer's gate policy to empty — after which `approval_gate.zig:96` auto-approves and no human is ever asked. That bypass needs no approval at all, so §1's removal of `approval_resolve` does not close it, and no narrowing of *which* tenant scopes are granted can: the capability the investigator needs and the capability that breaks the design are one capability.

Nor can a signed webhook substitute. The manual-route middleware verifies an HMAC over the request body (`fleet_runtime/webhook_verify.zig` `PROVIDER_REGISTRY`), and the investigator is a model holding `http_request`; `${secrets.NAME.FIELD}` substitutes a literal and cannot sign. The credential that fits is a fleet-bound `agt_a` key, which is not yet a principal (`docs/AUTH.md:362` — "Today this is a side door"), and which additionally requires `fleet:message` to be split out of `fleet:write` or it inherits the same problem at smaller scale.

So in this workstream a **human** wakes the repairer, from the diagnosis the investigator posts to Slack. Every other property — the gate, the repository-scoped mint, the git-computed revert, the refusal path, the critic — is proven without a machine credential existing anywhere in the crew. The automatic hop and its prerequisites move to M157_002, and until they land the crew is human-triggered by design rather than by omission.

- **Dimension 3.5** — **DONE** — No crew member holds a tenant API key; the repairer's wake carries a human actor → Test `test_crew_holds_no_tenant_key`
- **Dimension 3.6** — **DONE** — A credential holding `fleet:write` can blank a fleet's gate policy through PATCH — asserted as a *known* bypass so its closure in M157_002 is regression-tested rather than assumed → Test `test_fleet_write_can_blank_gate_policy`

- **Dimension 3.1** — **DONE** — The investigator's minted GitHub token carries `contents: read` and no `pull_requests` permission, so it can read history and cannot open a Pull Request → Test `test_investigator_token_is_read_only`
- **Dimension 3.2** — **DONE** — The shipped repairer bundle declares a non-empty gate rule → Test `test_repairer_bundle_declares_a_gate`
- **Dimension 3.3** — **DONE** — A repairer event without an approved gate yields no lease and no PR → Test `test_unapproved_event_opens_no_pr`
- **Dimension 3.4** — **DONE** — Denial and deadline expiry resolve terminally; the repairer never runs → Test `test_denied_or_timed_out_never_runs`

### §4 — The crew investigates, diagnoses, and proposes exactly one repair class

`library/incident-responder/` (investigator) wakes on a cron sweep, queries the customer's Grafana, correlates with recent repository history, posts a diagnosis to Slack, and — only when the cause is code-shaped and the repair is a revert of an identified commit — messages the repairer with repository, commit, and evidence. `library/incident-repairer/` produces one draft revert Pull Request (PR) and does nothing else: the reverted-to code was already green in Continuous Integration (CI), so no model authors any line of the change. Config-in-repo diffs and narrow patches are later rungs. Truth living only in a vendor console is recommended with a link, never written.

**The revert is computed by git, not by hand-rolled patch application.** Reconstructing a revert from the REST API means fetching each changed file's bytes at the parent commit and writing them back — which is only a revert if nothing else touched those files since. After an incident the base has usually moved, so that approach silently destroys unrelated work. `git revert` performs the three-way merge correctly and fails cleanly when it cannot.

The fetch is **on demand and daemon-executed**, modelled on the shipped credential-mint hook rather than on pre-fork materialization. Pre-fork is wrong twice over: it would fetch on *every* lease — an idle "hello" to the repairer would pay for a clone — and it would have to decide *what* to fetch before anything had parsed the request, meaning the daemon would scrape a repository and a commit out of model-authored prose. That is the same injection surface this spec refuses for `proposed_action`.

`lease_run.zig:59-62` already carries the correct pattern for `MintHook`: the child asks the daemon mid-run, and *"a child-supplied workspace is impossible — `cp.mint` sends only `lease_id`, and the daemon derives the workspace from it (Invariant 2)."* A fetch hook is that operation with a different payload. The child calls a tool naming repository and commit **as explicit arguments**; the daemon derives the workspace from `lease_id`, **validates the repository against the fleet's `repositories` binding and refuses anything outside it**, fetches depth-bounded into `{workspace}/repo/`, and answers ready. The child then reverts against a real working tree with no network and no credential.

That validation step is what makes the binding load-bearing rather than advisory: it narrows the mint *and* gates reads, so a misled repairer cannot even fetch a repository it is not bound to. Clone volume follows from the same design — a lease that needs no repository calls no tool and pays nothing, and the high-lease-rate fleet (the critic, ~5 leases per Pull Request as measured on `agentsfleet#586`) reads diffs over `http_request` and never needs a working tree at all.

**The fetch is bounded at the fetch, and nothing is cached.** A cross-lease repository cache was considered and rejected: `bundle_extract.zig:22,45-47` is the shipped precedent for a content-addressed cache that survives the per-lease `deleteTree`, and it has **no eviction** — it stays small only because every entry is capped at 4 MiB (`MAX_BUNDLE_TAR_BYTES`) and keyed by an immutable hash. A repository cache inherits the no-eviction property with none of the per-entry bound, converting a bounded and always-deleted per-lease cost into an unbounded and never-deleted host cost. So the per-lease `deleteTree` stays, and host disk is bounded by `worker_count` × one bounded fetch.

What that bound can be has a floor worth stating. Reverting commit `C` onto head `H` needs the trees and blobs of `C`, `C^`, and `H`, and no history walk — so a depth-bounded fetch of three explicit commits removes history but still costs roughly one checkout. Going below that means a blobless fetch plus a sparse checkout of only the paths `C` touched, with the daemon prefetching exactly those blobs — a lazy partial clone cannot work here, because materializing a missing blob would need network and a credential at revert time and the child holds neither. Change-sized rather than repository-sized is the target; three-commit depth-bounded is the floor this Section must not exceed.

**Orphaned workspaces are reaped at startup, because this Section is what makes them expensive.** Cleanup today is `defer cleanupWorkspace` (`lease_run.zig:107`), which does not run on `SIGKILL`, an out-of-memory kill, a panic, or a host reboot; `main.zig:91` only creates the storage home and never sweeps it. Today an unclean shutdown orphans ≤256 KiB of bundle support files and nobody notices. Once a workspace can hold a repository, the same shutdown orphans it permanently with no collector. At daemon startup no lease is held, so **every per-lease workspace under the storage home is orphaned by definition** — which makes the sweep trivially correct. What is *not* trivially correct is proving the swept directory is ours, because `RUNNER_STORAGE_HOME` is an operator-supplied string: a bare "delete every non-dot entry" lets a stray value reap host data, and one daemon per storage home is an assumption the `agt_r` token implies rather than a fact anything enforces. So the claim carries four proofs and reaps nothing unless all four hold — the path is canonicalized through the open handle and refused if too shallow to be a home; an exclusive advisory lock is held for the **process lifetime**, not for the sweep, so a rolling deploy's incoming daemon cannot reap the outgoing one's live leases; a sentinel marks the home, and the boot that writes it adopts without reaping, because a fresh home has no orphans to lose; and only a real directory named like a lease id is removed, so a dot-prefixed cache entry, a foreign name, and a lease-shaped symlink all survive.

- **Dimension 4.1** — A seeded regression yields a structured finding citing a real Grafana response digest, never an invented identifier → Test `eval_detection_cites_evidence`
- **Dimension 4.2** — The finding names the failing service and the correlated commit range → Test `eval_finding_names_service_and_commit`
- **Dimension 4.3** — Provider-outage and data-shaped incidents stay diagnosis-only: no repair intent sent → Test `eval_noncode_incidents_stay_diagnosis_only`
- **Dimension 4.4** — The repairer's PR is a revert of the named commit and touches nothing else → Test `eval_repair_is_a_revert`
- **Dimension 4.5** — **BLOCKED** (no `revert` operation exists — see Discovery, Aug 05) — A revert that does not apply cleanly to the target head is refused with a named reason; no branch is pushed and no model resolves the conflict → Test `test_conflicting_revert_refuses`
- **Dimension 4.6** — **DONE** — The fetch is depth-bounded, lands only in the lease's own workspace, and no credential reaches the child → Test `test_fetch_is_bounded_and_credential_free`
- **Dimension 4.6a** — **DONE** — A lease that asks for no repository fetches nothing; a fetch for a repository outside the fleet's binding is refused → Test `test_fetch_is_on_demand_and_binding_scoped`
- **Dimension 4.7** — Cold install of both bundles onto a fresh workspace succeeds with declared credentials and hosts → Test `test_cold_install_from_library`

**Both bundles carry three authoring obligations the runtime cannot enforce.**

First, **honest degradation at the context threshold — not continuation, because continuation does not exist.** `runner_progress.zig:235-250` watches `prompt_tokens / context_cap` after every model round-trip, but NullClaw exposes no mid-loop interrupt — *"we observe instead of force"* — so the runtime logs the crossing and nothing more. The re-enqueue half was dropped in the fleet-runner split and its scaffolding was left standing: `.continuation` is a first-class `EventType` (`event_envelope.zig:33`), `continuationActor` computes a flat non-nesting actor (`:88`), and `event_rows.zig:84-95` lifts `original_event_id` onto `resumes_event_id` — yet `continuationActor` has **zero callers**, and `service_report.zig:5-6` states it outright: *"continuation is a no-op on the happy path."* A fleet ending with `content='needs continuation'` is therefore XACK'd, its affinity slot released, and nothing re-enqueues it. Even a re-enqueue would not resume: a new lease mints a new `lease_id` and therefore a new workspace, and `core.fleet_sessions.context_json` is loaded by `claimFleet` and never placed on `LeasePayload`, so the run would restart with `SKILL.md` plus a note. So the obligation these bundles carry is to **end with a named degradation** stating what was and was not read — never to promise a continuation the runtime cannot deliver. Finishing continuation is M157_003 (Out of Scope).

Second, **escalation memory**: an incident stays broken while its repair is parked, so every subsequent sweep re-finds it. The investigator records what it has escalated and suppresses a repeat while that incident is outstanding — otherwise a single incident produces one approval request per sweep interval, all queued behind the first. **This requires a bundle change the current bundle contradicts:** `library/incident-responder/TRIGGER.md` declares `tools: [http_request]` and nothing else, so `memory_store` and `memory_recall` are never built and the dedup cannot run as specified. Both tools join the investigator's declared list.

Third, **an explicit `tools:` list on both bundles**, because a bundle only gets the tools it names. `runner_helpers.zig:242-243` falls back to `hosted_tools.buildDefault` when `tools` is absent *or* not an array, and that is `allTools` filtered only against `UNSUPPORTED_HOSTED_TOOLS` — the seven cron/schedule names (`tool_bridge.zig:40-48`). An omitted list therefore silently yields NullClaw's entire set rather than the crew's intended surface, which makes what a fleet can do depend on a field nobody wrote. The repairer names `git`, `http_request`, and the memory family it needs to remember what it has already opened; the investigator names `http_request` plus `memory_store` and `memory_recall`.

- **Dimension 4.8** — **DONE** — Both bundles instruct a named degradation at the context threshold, and neither promises continuation → Test `test_bundles_declare_degradation`
- **Dimension 4.9** — A second sweep over an already-escalated, still-broken incident sends no second wake → Test `eval_escalation_is_deduped_by_memory`
- **Dimension 4.10** — **DONE** — Both bundles declare an explicit `tools:` array; an omitted list would expose the full default set → Test `test_bundles_declare_explicit_tools`
- **Dimension 4.11** — A replayed repair intent for a commit the repairer already opened a Pull Request for opens no second one → Test `eval_repairer_dedupes_by_memory`
- **Dimension 4.12** — **DONE** — A workspace orphaned by an unclean shutdown is reaped at daemon startup; the dot-prefixed bundle cache is not → Test `test_startup_sweep_reaps_orphans`

### §4a — A crew installs from local markdown, without borrowing a template

`POST /fleet-libraries` already accepts `source_kind:"upload"` with inline `skill_markdown` + `trigger_markdown`, content-addressed with `ON CONFLICT DO NOTHING` (`fleet_bundles/resolve.zig:81`). No surface reaches it: the dashboard's `AddLibraryDialog.tsx` hardcodes `source_kind: SOURCE_KIND_GITHUB` with an `owner/repo` input, and the CLI has no verb for it. So the only way to obtain a fleet row today is to install *some* library entry — which is why hand-setup installs `github-pr-reviewer` and immediately overwrites both of its markdown files. Nothing of the template survives on the investigator or the repairer; it is a vehicle, not a choice.

Exposing the shipped path costs **zero daemon code** and removes the dance. The dashboard gains an upload source beside the GitHub one; the CLI gains the matching verb so the crew is reproducible from a checkout.

- **Dimension 4a.1** — The dashboard offers an upload source and posts `source_kind:"upload"` with both markdown bodies → Test `test_dashboard_uploads_local_bundle`
- **Dimension 4a.2** — The CLI creates a library entry from a local bundle directory, and `install --library <it>` yields a fleet whose markdown matches the source byte-for-byte → Test `test_cli_uploads_and_installs_local_bundle`
- **Dimension 4a.3** — **DONE** — Re-uploading identical markdown is content-addressed to the same entry rather than duplicating it → Test `test_upload_is_content_addressed`

### §5 — Data-plane credentials and library publication use only existing mechanisms

Grafana and Elastic keys are plain workspace secrets (never registry entries, per `connectors.md`), reaching the run only as `${secrets.NAME.FIELD}` placeholders substituted at the tool bridge. Bundles publish through the existing admin library flow (`draft` → `public`, content-addressed snapshot).

**Egress is bounded twice, and only one of the two rings is tool-shaped.** `ctx.policy` is read by `buildHttpRequest` and no other builder (`tool_builders.zig:183`), and `secret_substitution` is reachable only from `policy_http_request.zig` — so *credential substitution* and the tool-level host check bind `http_request` alone. The outer ring does not: `network/Plan.zig` derives a per-lease egress plan enforced by a network namespace — a veth pair on a point-to-point `/30`, a static `/etc/hosts` carrying only allowlisted names, and a **neutered `/etc/resolv.conf`**. A host off the allowlist has neither name resolution nor a route, for `git` exactly as for `http_request`. The filesystem is fenced the same way, by Landlock: workspace read-write, system paths read-execute, everything else denied — so one lease cannot read another's workspace, and the daemon derives that path from `lease_id` because the child cannot supply one (`lease_run.zig:61-62`).

That asymmetry is why §4's revert fetches through the daemon's on-demand hook rather than granting `git` a credential: the outer ring already bounds where git may go, so the only thing missing would be a token — and a daemon-executed fetch means no token is needed inside the sandbox at all.

- **Dimension 5.1** — Grafana/Elastic secrets stay placeholders in prompt and logs; raw bytes appear only in the egress request → Test `test_data_plane_secrets_stay_placeholders`
- **Dimension 5.2** — A host outside a bundle's allowlist is refused for that bundle's leases → Test `test_undeclared_host_refused`
- **Dimension 5.3** — Onboard → publish → workspace-visible → installable, via the existing admin flow → Test `test_bundles_publish_and_list`

### §6 — The benchmark is honest by construction

`bench/incident-response/` seeds an instrumented corpus and injects incidents from seed manifests split into disjoint calibration and evaluation sets. The threshold baseline is tuned on calibration only, then frozen by config hash. Detection scores only when a structured result names the affected service and incident class within tolerance — "anomaly found" scores zero. **This harness measures detection over a synthetic corpus; it does not exercise the crew, the gate, or the write path, and no rubric row claims that it does.**

- **Dimension 6.1** — **DONE** — The injector is reproducible: identical seed manifest → identical corpus hash → Test `test_injector_deterministic`
- **Dimension 6.2** — **DONE** — Calibration and evaluation manifests are disjoint; scoring refuses a mixed corpus → Test `test_seed_manifests_disjoint`
- **Dimension 6.3** — **DONE** — The baseline is frozen: scoring refuses a baseline whose config hash drifted after calibration → Test `test_baseline_frozen`
- **Dimension 6.4** — **DONE** — Scoring requires service + class within tolerance; unstructured claims score zero → Test `test_scoring_requires_service_and_class`
- **Dimension 6.5** — **DONE** — The report emits the full metric set, including variance, cost, and threshold-win cases → Test `test_report_metrics_complete`

### §7 — The demo topology runs on AWS and the stage proof is replay-safe

A playbook stands up a small multi-service instrumented workload on EC2, Grafana receiving its telemetry, an `agentsfleet-runner` host, failure-injection scripts, and both fleets installed by hand. **The runner host installs `git`** — §4's revert rung shells out to the real binary because no API computes a three-way merge, and the daemon resolves it from an absolute-path allowlist rather than `$PATH`; its absence is a named refusal (`git_unavailable`), so a host without it fails every repair loudly instead of subtly. **The runner host sets `RUNNER_STORAGE_HOME` to real disk.** Its default is `/tmp/agentsfleet-runner` (`runner/daemon/config.zig:129`), and `/tmp` is tmpfs on most hosts — memory, not disk. That default was harmless while a workspace held only import-capped bundle support files; §4 puts a repository working tree there, so leaving it on tmpfs charges every fetch against host memory. The stage proof: inject a held-out regression live, watch detection → diagnosis → Slack approval naming the commit → one draft revert PR, then replay the same investigator message and show the second run parks on its own approval rather than opening a second PR.

- **Dimension 7.1** — The playbook's check mode is idempotent: two consecutive runs both exit clean → Test `playbook_check_idempotent`
- **Dimension 7.2** — An injected failure traverses the collector → Grafana and is detected end-to-end on the live stack → Test `e2e_injected_failure_detected`
- **Dimension 7.3** — Approving once produces exactly one draft PR on the live stack → Test `e2e_single_pr_on_approval`
- **Dimension 7.4** — A replayed investigator message parks a second gate and opens no second PR without a second approval → Test `e2e_replay_parks_not_writes`

## Interfaces

```
Scope grant (auth/scopes.zig): DefaultGrant gains a machine-credential variant whose
  members are the .tenant set minus .approval_resolve. defaultScopes(<machine>) feeds
  tenant_api_key.zig; defaultClaim(.tenant) still feeds identity_events_clerk.zig's
  DEFAULT_SIGNUP_SCOPES unchanged. The scope itself is untouched — only which
  credential source is granted it.
ActionDetail (fleet_runtime/approval_gate.zig), populated by fleet/approval_gate.zig:
  { tool, action, params_summary, gate_kind, proposed_action, evidence,
    blast_radius, timeout_ms }. Two sources, deliberately separated:
    gate_kind + blast_radius  ← the matched GateRule (workspace-authored config)
    proposed_action + evidence ← the triggering event (MODEL-authored prose)
  The model half renders as an attributed claim, never as a system statement,
  beside daemon-derived facts it cannot forge (fleet id, actor, event id).
  Never diff bytes, never secret material.
Fleet repository binding (x-agentsfleet frontmatter, top level, NOT under triggers):
  repositories: ["owner/repo", …] — the egress scope for this fleet's credentials.
  Distinct from the webhook trigger's `repositories`, which is an INGRESS binding
  (which repos may wake the fleet) and must not be overloaded. Absent → fail closed.
Fleet repository access (x-agentsfleet frontmatter, top level, beside `repositories`):
  repository_access: read | write — read mints { contents: "read" }; write mints
  { contents: "write", pull_requests: "write" }. Absent → fail closed (no mint),
  same as an absent `repositories` binding. Investigator declares read, repairer write.
GitHub mint (credentials/integration_github.zig): POST /app/installations/{id}/access_tokens
  body { repositories: [<fleet binding>], permissions: <per repository_access above> }
  — absent binding or absent access level → no mint (fail closed).
Investigator → repairer edge: POST /v1/workspaces/{ws}/fleets/{repairer}/messages with a
  tenant API key (scope fleet:write). The message carries repository, suspect commit,
  and evidence links — 8 KiB cap, so identifiers and links only, never file contents.
Repair rung: revert only, computed by git rather than by hand-rolled patch application.
  On demand mid-run, the daemon fetches the suspect commit, its parent, and the
  target head (depth-bounded, no history) into the per-lease workspace; the sandboxed
  child runs the revert with no network and no credential; a revert that does not
  apply cleanly is REFUSED, never resolved by the model. No model-authored source
  lines exist in the diff. Nothing is cached across leases and the workspace is
  deleted at run end, so host cost is bounded by worker_count × one bounded fetch.
Library upload (already shipped, unreachable): POST /fleet-libraries accepts
  source_kind:"upload" with inline skill_markdown + trigger_markdown. Exposing it on
  the dashboard and the CLI removes the borrowed-template install dance.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Machine attempts approval | A fleet or external service resolves a gate with a tenant key | Route refuses on scope; structured log; the gate stays pending until a human decides |
| Repairer bundle without a gate rule | Hand-install omits `gates.rules` | Bundle test fails at build; on a live workspace the omission is visible as an auto-approved run in the activity stream |
| Denied / timed out | Human denies, or the gate deadline passes | Terminal status; the repairer's lease is never issued; diagnosis artifacts remain |
| Unbound repository | Repairer fleet declares no top-level `repositories` binding | Mint refuses; the run reports it could not authenticate rather than reaching a wrong repository |
| Revert does not apply | Target head moved and something else touched the same files | Refused with a named reason before any push. The model never resolves the conflict — that would forfeit "no model-authored source lines", the only property that makes revert the safe first rung. A fresh sweep may propose again against the new head |
| Fetch fails | Repository unreachable, commit garbage-collected, or depth bound insufficient | The fetch hook answers rejected; the run reports what it could not read and produces no repair. No partial tree is left for a later step to misread |
| Fetch outside the binding | Repairer asks for a repository its `repositories` list does not name | Daemon refuses before any network call; the ask is logged with the fleet id. A misled repairer cannot read outside its scope, let alone write |
| Approval left unattended | Nobody resolves a parked gate | The gate's own timeout expires it, and only then does the fleet's next event become reachable — the Pending Entries List is re-delivered ahead of newer entries (`assign.zig:213-217`), so one stale approval blocks that fleet's queue for the whole timeout. The repairer therefore sets a short timeout rather than inheriting the 24-hour default |
| Duplicate investigator message | Retried or double-delivered steer | Each message parks its own gate; a second PR requires a second human approval. No caller idempotency key exists — the gate is the bound |
| Data plane unreachable or secret missing | Grafana down mid-sweep, or a declared credential absent | Finding degrades honestly (names what it could not read); no repair intent sent; existing stop-the-tool-call codes in the activity stream |
| Upstream write failure | GitHub rejects branch or PR creation | The repairer's tool call fails with the vendor's response class; the run reports it; nothing partial is claimed as done |
| Seed drift | Benchmark run over a corpus whose hash mismatches the manifest | Harness refuses to score; names both hashes |

## Invariants

1. No machine credential can resolve an approval, and no human loses the ability to — the machine grant excludes `approval_resolve` while the signup claim retains it, asserted by a set-difference unit test and by a route-level integration test.
2. The investigator cannot write to GitHub — its bundle declares `repository_access: read`, so the daemon mints a token carrying `contents: read` and no `pull_requests` permission. The vendor refuses the write regardless of what the model attempts, and the mint is the authoritative gate rather than the prompt.
3. No repairer lease is issued for an event whose gate is not approved — the existing pre-lease check is the only path, and this workstream adds no bypass.
4. Every parked approval names its proposed action, evidence, and blast radius — a blank `ActionDetail` field is a test failure, not a display default.
5. A minted GitHub token reaches only the repositories the fleet declared — the mint body pins them, and an unbound fleet mints nothing.
6. Raw secret bytes never appear in prompt, result, or logs — existing tool-bridge substitution re-asserted by test for the new credential names.
7. Benchmark evaluation incidents never inform tuning — calibration and evaluation manifests are disjoint by construction and the scorer enforces it.
8. No model resolves a merge conflict — a revert that does not apply cleanly is refused before any push, so every shipped diff is git's inverse patch and contains no model-authored source line.
9. No credential enters the sandbox for repair work — the fetch is daemon-executed on the child's demand (never pre-fork, which §4 rejects: it would fetch on every lease and would have to choose a repository before anything had parsed the ask), the credential rides the git process's environment, and the child receives a working tree, never a token.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `repair_intent_sent` | ops | Investigator messages the repairer | investigator fleet id, repo, commit, evidence kinds | no secrets, no file contents | `eval_finding_names_service_and_commit` |
| `repair_approval_requested` | ops | Gate parks a repairer event | gate action id, repo, commit | detail fields only, no payloads | `test_gate_detail_is_populated` |
| `repair_approval_resolved` | ops | Gate resolves approve/deny/timeout | action id, resolution, actor kind | no actor PII beyond existing gate fields | `test_denied_or_timed_out_never_runs` |
| `repair_pr_opened` | ops | Repairer opens the draft PR | repo, pr url, reverted commit | no diff bytes | `e2e_single_pr_on_approval` |
| `machine_approval_refused` | ops | A machine credential is refused at the resolve route | principal mode, route | no key material | `test_api_key_cannot_resolve_approval` |
| `benchmark_run_completed` | ops | Harness finishes a scored run | corpus hash, metric summary | aggregate numbers only | `test_report_metrics_complete` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_machine_grant_excludes_approval_resolve` | machine-grant set lacks `approval_resolve`, retains the other ten tenant capabilities |
| 1.2 | unit | `test_signup_claim_retains_approval_resolve` | `defaultClaim(.tenant)` still contains `approval:resolve`; set difference against the machine grant is exactly that one member |
| 1.3 | integration | `test_api_key_cannot_resolve_approval` | `agt_t` bearer at resolve route → refused; user JWT → accepted |
| 1.4 | unit | `test_no_machine_approval_callers` | `requiredScopes(workspace_approval_resolve, POST)` vs each default grant → machine and runner fail `satisfiesAny`, owner passes; same for the inbox-read rung |
| 2.1 | integration | `test_gate_detail_is_populated` | parked gate row carries non-empty proposed_action/evidence/blast_radius |
| 2.2 | unit | `test_slack_approval_names_the_action` | built message contains repo, commit sha, and the draft-PR outcome string |
| 2.3 | unit | `test_mint_body_is_repository_and_access_scoped` | `repository_access: write` → body carries the declared repo + contents/pull_requests; `read` → contents:read only, no pull_requests key |
| 2.4 | unit | `test_unbound_fleet_mints_nothing` | fleet with null repositories → mint refused, no token returned |
| 3.1 | integration | `test_investigator_token_is_read_only` | investigator mint → token with `contents: read`, no `pull_requests`; a PR-create call with it is refused by the vendor |
| 3.2 | unit | `test_repairer_bundle_declares_a_gate` | shipped TRIGGER.md parses to a non-empty `gates.rules` |
| 3.3 | integration | `test_unapproved_event_opens_no_pr` | repairer event, gate pending → no lease issued, fake GitHub sees zero calls |
| 3.4 | integration | `test_denied_or_timed_out_never_runs` | deny and deadline expiry → terminal, repairer lease never issued |
| 4.1 | eval | `eval_detection_cites_evidence` | seeded regression → finding cites a returned Grafana digest |
| 4.2 | eval | `eval_finding_names_service_and_commit` | traced failure → service + commit range named |
| 4.3 | eval | `eval_noncode_incidents_stay_diagnosis_only` | provider-outage seed → no repair intent message sent |
| 4.4 | eval | `eval_repair_is_a_revert` | repairer output diff equals the revert of the named commit |
| 4.5 | integration | `test_conflicting_revert_refuses` | head moved with a conflicting edit to the same file → refused with a named reason, zero pushes, no model conflict resolution |
| 4.6 | integration | `test_fetch_is_bounded_and_credential_free` | six-commit `file://` remote → three commits fetched (depth 2 over two tips), `.git/shallow` present, the workspace holds only the daemon-created `repo`, and neither the token nor its base64 spelling appears in any path or byte of the tree |
| 4.6 | unit | `test_fetch_is_bounded_by_bytes_not_history` | a child that writes past the byte ceiling and then lingers is killed on the SIZE bound, with the time bound nowhere near — depth bounds history, this bounds bytes |
| 4.6a | integration | `test_fetch_is_on_demand_and_binding_scoped` | lease asking for no repository → zero fetches; repository outside the binding → refused before any network call |
| 4.7 | e2e | `test_cold_install_from_library` | fresh workspace + published entries → both installed, scheduled, policy attached |
| 4.8 | unit | `test_bundles_declare_degradation` | both `SKILL.md` bodies name a degradation path at the context threshold; neither contains the string `needs continuation` |
| 4.9 | eval | `eval_escalation_is_deduped_by_memory` | second sweep over an outstanding escalation → zero repair-intent messages sent |
| 4.10 | unit | `test_bundles_declare_explicit_tools` | both `TRIGGER.md` files parse to a non-empty `tools` array — an absent or non-array value would resolve `hosted_tools.buildDefault` |
| 4.11 | eval | `eval_repairer_dedupes_by_memory` | replayed repair intent naming a commit the repairer already opened a Pull Request for → zero second Pull Requests; the repairer recalls before it proposes. **Memory is the courtesy, the gate is the bound** — a crash between the vendor call and `memory_store` loses the record, and the second attempt is then stopped by needing a second human approval, not by memory |
| 4.12 | unit | `test_startup_sweep_reaps_orphans` | seeded `{storage_home}/<uuid>/` removed after sweep; `{storage_home}/.bundle-cache/` survives |
| 4a.1 | integration | `test_dashboard_uploads_local_bundle` | upload source posts `source_kind:"upload"` with both markdown bodies → entry created |
| 4a.2 | e2e | `test_cli_uploads_and_installs_local_bundle` | local bundle dir → library entry → installed fleet whose markdown matches the source byte-for-byte |
| 4a.3 | unit | `test_upload_is_content_addressed` | identical markdown twice → one entry, second call is a no-op |
| 5.1 | integration | `test_data_plane_secrets_stay_placeholders` | prompt/log capture free of raw bytes for grafana/elastic names |
| 5.2 | integration | `test_undeclared_host_refused` | non-allowlisted host → tool call refused with existing code |
| 5.3 | integration | `test_bundles_publish_and_list` | onboard → draft → public → visible + installable |
| 6.1 | unit | `test_injector_deterministic` | same manifest twice → identical corpus hash |
| 6.2 | unit | `test_seed_manifests_disjoint` | overlapping ids → scorer refuses |
| 6.3 | unit | `test_baseline_frozen` | baseline hash drift post-calibration → scorer refuses |
| 6.4 | unit | `test_scoring_requires_service_and_class` | unstructured "anomaly" claim → score 0 |
| 6.5 | unit | `test_report_metrics_complete` | report contains every §6 metric incl. threshold-win cases |
| 7.1 | e2e | `playbook_check_idempotent` | check mode twice → both exit 0 |
| 7.2 | e2e | `e2e_injected_failure_detected` | live injected failure → detection event within sweep bound |
| 7.3 | e2e | `e2e_single_pr_on_approval` | one approval → exactly one draft PR |
| 7.4 | e2e | `e2e_replay_parks_not_writes` | replayed message → second gate pending, PR count stays 1 |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A machine credential cannot approve, and the human signup claim still can (§1) | `make test-integration TEST_FILTER='approval'` then `make test-unit-all TEST_FILTER='grant'` | exit 0 both; `test_api_key_cannot_resolve_approval` and `test_signup_claim_retains_approval_resolve` listed as pass | P0 | |
| R2 | The approval names its action and the token is repository-scoped (§2) | `make test-unit-all TEST_FILTER='gate_detail\|mint_body'` | exit 0, both tests listed by name | P0 | |
| R3 | No unapproved event produces a write (§3) | `make test-integration TEST_FILTER='repairer'` | exit 0, `test_unapproved_event_opens_no_pr` listed as pass | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R5 | Cold install of both published bundles on a fresh workspace (§4–§5) | `make test-integration TEST_FILTER='cold_install'` | exit 0 | P0 | |
| R6 | Detection benchmark is reproducible — **detection only, not the crew** (§6) | `make bench-incident SEED_MANIFEST=eval` run twice | identical corpus hash line both runs | P1 | |
| R7 | The retired repair kernel leaves no orphan | `rg -n 'repair_proposal\|repair_bounds\|UZ-REPAIR' src/ \|\| true` | zero hits | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

**Test Delta note (VERIFY):** this diff **deletes 13 registered tests** with the repair kernel. The Test Delta row is measured against the baseline *after* subtracting those; a flat or negative raw count is expected and is not, on its own, grounds to return to EXECUTE. The added tests in §1–§4 must exceed 13 for the delta to be genuinely positive.

### Behaviour evals

- **Grounding rule:** every finding cites only identifiers and digests actually returned by Grafana or the repository — a fabricated identifier is a P0 ❌.
- **Golden set:** `bench/incident-response/seeds/` — 12 evaluation incidents + 12 clean windows across four classes (obvious spike, slow burn, cross-service failure, deployment-correlated regression — the nightmare case) plus non-code seeds for 4.3.
- **Ship threshold:** grounding 100% · structured detection pass ≥ 75% on the evaluation set · 0 critical failures on the deployment-correlated regression class.
- **Fallback:** low confidence or unreadable data plane → diagnosis-only (named degradation, no repair intent); fabricated output is a P0 ❌.

## Dead Code Sweep

Four files and one error family leave in this diff, all of them substrate for the daemon-side apply that no longer exists:

| Path | Lines | Why it goes |
|---|---|---|
| `src/agentsfleetd/fleet/repair_proposal.zig` | 295 | Canonical hashing bound approved bytes to applied bytes; no daemon apply site remains to compare at |
| `src/agentsfleetd/fleet/repair_proposal_test.zig` | 280 | Tests of the above |
| `src/agentsfleetd/fleet/repair_bounds.zig` | 131 | Apply-time diff-path bounds; the repairer writes through the vendor API, so no diff is inspectable before the write |
| `src/agentsfleetd/fleet/repair_bounds_test.zig` | 114 | Tests of the above |
| `UZ-REPAIR-001..005` | — | Every code names an apply-service failure (stale base, bounds, duplicate, upstream, malformed proposal) with no site left to raise it |

Removal is verified by rubric R7 rather than by inspection.

## Out of Scope

- **Fleet identity, and with it the autonomous hop — M157_002.** `fleet:message` split out of `fleet:write` so waking a fleet stops implying the right to reconfigure it; first-class `agt_a` fleet keys (`AuthMode.fleet_key` + middleware branch, per `docs/AUTH.md:362`); `actor=chain:<fleet_id>` on machine hops; a hop cap; and a caller idempotency key on `POST /messages`. **This is a prerequisite for the crew being autonomous, not an enhancement of it** — until it lands, any credential that could wake the repairer could also blank its gate, so this workstream keeps a human on the wake and proves everything else. The Discovery table records the bypass, and Dimension 3.6 asserts it, so its closure is regression-tested rather than assumed.
- **Finishing continuation, and the cold-start economics around it — M157_003.** Three findings in this workstream's Discovery share one home and none belong here. (a) `continuationActor` has no caller and the report path calls continuation *"a no-op on the happy path"*, so the `.continuation` EventType and `resumes_event_id` linkage are scaffolding without a producer. (b) `core.fleet_sessions.context_json` is written every report and never placed on `LeasePayload`, so it is a bookmark nobody reads — either wire it as an additive defaulted field beside `instructions`, or delete it and let the table mean only "is this fleet executing". (c) `cache_control` appears nowhere in `src/` or the vendored engine while the rate tables and lease row already price and meter cached input, so a fleet re-pays full input price for its whole `SKILL.md` on every lease. Wiring (a) must decide explicitly that the **workspace does not survive** the hop — it is derived from `lease_id` and Landlock-fenced, and that property is not negotiable. Until this lands, bundles end with a named degradation rather than promising a continuation the runtime cannot deliver (Dimension 4.8).
- Repair rungs beyond revert — config-in-repo diffs and narrow patches need their own bounds story and their own spec.
- Chat-to-fleets authoring of a crew — prove the shape by hand first.
- Automatic merge, deploy, rollback, or any write beyond the one draft PR.
- Elastic as a data plane — Grafana is first; Elastic follows once the loop is proven.
- Jira post-mortem tickets — optional and later.
- Website retheme around this wedge — separate spec after the loop is proven live.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens Slack to a diagnosis naming the failing service and the suspect commit, and an approval that says "revert `abc123` in `owner/repo`, opens one draft Pull Request". They click Approve and GitHub shows exactly that.
2. **Preserved user behaviour** — Existing fleets, triggers, approvals, and the platform-ops diagnosis flow are untouched; a workspace that installs neither bundle sees one change only: a tenant API key can no longer resolve an approval.
3. **Optimal-way check** — Direct: the loop reuses the gate, the mint, the tool bridge, the library, and the message edge. The three code changes are each a single-purpose correction to a mechanism that already exists. Gap to optimal: approval authorises a bounded *run*, not specific bytes, so the human approves an intent and reviews the result on the draft PR. That is the honest shape of a gate that binds before a lease.
4. **Rebuild-vs-iterate** — Iterate; every needed substrate exists. The one thing that was genuinely missing — a reason the investigator *cannot* write — comes from credential separation across two fleets rather than from new code.
5. **What we build** — One scope-grant correction, one approval-detail threading, one mint narrowing, two bundles, a demo playbook, and the scenario-doc flip.
6. **What we do NOT build** — Everything in Out of Scope; notably no daemon-side apply service, no proposal record, and no second approval mechanism. **The repairer is a model run holding a write credential, deliberately.** That is bounded by three things and no others: it cannot run without an approved gate, its token reaches only the declared repositories, and its only rung is a revert of code CI already passed.
7. **Fit with existing features** — Compounds with approvals, connectors, library, schedules, and the event log. Must not destabilize: the approval gate — a populated `ActionDetail` must never change gate outcomes, only what the human reads.
8. **Surface order** — Runtime + bundles first; the dashboard shows nothing new beyond a richer gate card. Command-line and web surfaces unchanged.
9. **Dashboard restraint** — No new approval UI: the existing gate card carries the newly-populated detail fields; the diff is reviewed on the draft PR, where code review already lives.
10. **Confused-user next step** — A refused machine approval names the scope it lacked; a degraded sweep says what it could not read; a repairer that cannot mint says which repository binding is missing.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven Sections in one Workstream: the human gate made true (§1) before the approval is made legible and the token bounded (§2), before the structural separation that relies on both (§3); the bundles (§4), publication (§5), benchmark (§6), and topology (§7) prove it. One PR carries the provable loop.
- **Alternatives considered:** (a) **Daemon-side deterministic apply with a content-addressed proposal record** — the original §1/§2, partially built and removed here. It offers a strictly stronger guarantee (approved bytes are shipped bytes) but requires a proposal table, a store, an apply service, and a report-path hook, and it does not match the crew shape the product needs. Superseded, not refuted. (b) **A method-and-path allowlist at the egress boundary** (`PolicyHttpRequestTool`) to bound what the repairer may call — rejected as unnecessary once credential separation puts the write in a different fleet; branch and tree endpoints permit arbitrary branch content anyway, so the bound it buys is "cannot land", which the gate already provides. (c) **Routing the investigator → repairer hop through Slack** — rejected: `slack/events.zig` resolves `(team, channel)` to the channel's *resident* fleet, so the repairer would wake on every human message in the channel, handing a prompt-injection path to the fleet holding the write token. (d) **Collapsing investigator and repairer into one fleet** — rejected: one fleet holds one credential set across both leases, so "read-only first, write second" would be prompt-shaped rather than structural.
- **Alternatives considered (repair mechanism):** (e) **Reconstructing the revert over the REST API** — fetch each changed file at the parent commit and write those bytes back. Rejected: that is only a revert when nothing else touched those files since, and after an incident the base has usually moved, so it silently destroys unrelated work. git's three-way merge is correct and fails cleanly; hand-rolling it is the kind of subtle wrongness that ships green. (f) **Granting `git` a credential inside the sandbox** by extending `secret_substitution` past `policy_http_request.zig` — rejected for this workstream: it puts a live token where a model that may have been talked into something can read it, and the pre-fork fetch achieves the same outcome with no token in the blast radius at all. It remains the right long-term move for repair work that genuinely needs authenticated in-sandbox git, and belongs in its own spec with its own review.
- **Patch-vs-refactor verdict:** this is a **patch** — three small corrections to shipped mechanisms, one shipped-but-unreachable path exposed, and two markdown bundles. Solution-size matches problem-size, and the deletion of the apply substrate makes the diff net-simpler.

## Discovery (consult log)

- **Consults** — Architecture: `scenarios/production-deploy-repair.md` is rewritten in the same diff to describe the gate-bound two-fleet design (`name_architecture` satisfied). Adversarial review: Codex CLI session (Aug 01, 2026) — verdict GO on the problem pick; its finding that the approval gate binds at lease rather than at tool time is now the load-bearing design fact rather than a caveat.
  - > Indy (2026-08-01): "How about we focus on the logs fo grafana, elastic and git to push a PR? since that is my need now as well?" — context: choosing the wedge this spec implements.
  - > Indy (2026-08-01): "yes pressure test the pick with codex" → "go" — context: adversarial review accepted; spec authoring authorized.
- **Metrics review** — six operator events (table above); no product-analytics funnel touched.
- **Skill-chain outcomes** — empty at reconciliation; populated at VERIFY/REVIEW/CHORE(close).
- **Deferrals** — M157_002 (fleet identity and provenance), Indy-acked below.

### Aug 03, 2026 design session — the use case restated, and the shape redesigned

Design-only session. Four reviews ran: `plan-ceo-review`, `plan-eng-review`, an independent
adversarial subagent, and Codex CLI 0.146.0 (`gpt-5.6-sol`, reasoning effort xhigh).

**Indy restated the use case: autonomous observability patching.** The signals are the
**customer's**, in the customer's Grafana and Elastic, plus Fly.io logs — not agentsfleet's
own telemetry. A customer signs up, connects their tools, and a composite of fleets (a
"crew") watches their logs, senses an outage or anomaly, ascertains what class of repair
applies, and produces a reviewable Pull Request (PR). Grafana is the first source; Elastic
second. A Jira ticket carrying the post-mortem is optional and later.

**Settled by Indy (do not re-litigate):**

- Everything is a fleet in a sandbox. No new "broker" abstraction.
- Security surface of a fleet holding write credentials: **accepted**; see the correction
  below on how large that surface actually was.
- All fleets reach the language model; no per-member gating.
- `github-pr-reviewer` stays as the one GitHub-sourced template; other members are set up
  manually via Command-Line Interface (CLI) or API for now.
- Chat-to-fleets authoring is **later**. Prove the shape by hand first.
- No normalized schema for any crew artifact; a generated markdown document is acceptable.
- M154 is **not** a blocker for this work — confirmed again on Aug 03 after a false alarm
  from the reconciling agent; see the correction below.

**Verified in code — cite these rather than re-deriving:**

| Finding | Evidence |
|---|---|
| A tenant API key carries `approval_resolve`, `apikey_admin`, `secret_write`, `workspace_admin` from a **comptime-pinned** grant; `core.api_keys` has no scopes column. **A fleet holding a tenant key can resolve its own approval.** §1 fixes this by splitting the grant. | `auth/scopes.zig:99-110`, `auth/middleware/tenant_api_key.zig:119` |
| `grantMembers(.tenant)` has **two** consumers, not one: `defaultScopes` → machine `agt_t` principals, and `defaultClaim` → `DEFAULT_SIGNUP_SCOPES` written to Clerk `publicMetadata` at signup, which becomes the human tenant owner's `scopes` claim. Trimming the shared list would have stripped approval rights from every new tenant owner. The reconciling agent proposed exactly that before checking the second consumer. | `auth/scopes.zig:118-125`, `http/handlers/auth/identity_events_clerk.zig:41` |
| The GitHub App installation token is minted with `.body = ""` — **no `repositories`, no `permissions`** — so any fleet declaring the `github` credential receives full App permissions across every repository in the installation for an hour. Found Aug 03; neither prior review considered it. §2 fixes it. | `credentials/integration_github.zig:72-77` |
| The approval gate checks policy **before a lease is issued** and parks the event; there is no mid-run hold. `ActionDetail`'s `gate_kind`/`proposed_action`/`evidence`/`blast_radius` are built blank, and the code comment records them as designed-but-unthreaded. §2 threads them. | `fleet/approval_gate.zig:1-7,112-127` |
| `GateDecision` falls through to `.auto_approve` when no rule matches, and `GatePolicy.rules` defaults to empty — **gating is opt-in per fleet**. A bundle shipped without `gates.rules` is an autonomous agent. §3 tests against it. | `fleet_runtime/approval_gate.zig:96`, `config_gates.zig:67-68` |
| `ctx.policy` is read by `buildHttpRequest` and **no other builder** — `${secrets.NAME.FIELD}` substitution and `network.allow` bind `http_request` only; `git`/`shell` get the placeholder literally and no egress allowlist. Write through the GitHub HTTP API; **no runner checkout is needed and none should be built.** | `src/runner/engine/tool_builders.zig:183` |
| Slack events resolve `(team, channel)` to the channel's **resident** fleet, materializing on miss — they do not route to an arbitrary fleet by trigger source. Routing the crew handoff through Slack would wake the repairer on every human message in the channel. | `http/handlers/connectors/slack/events.zig:26,177` |
| The runner deletes the per-lease workspace after every run, so fleets **cannot hand each other files**; messages cap at 8 KiB. Evidence travels as links and identifiers. | `src/runner/daemon/lease_run.zig:99`, `http/handlers/fleets/messages.zig:30` |
| `POST /v1/workspaces/{ws}/fleets/{id}/messages` (scope `fleet:write`) is the only fleet-to-fleet edge. It has **no caller idempotency key** and machine hops log `actor=steer:`, losing parent-fleet provenance. No hop bound exists. Deferred to M157_002; the gate bounds the consequence in the meantime. | `http/route_template.zig:75`, `http/route_scopes.zig:153` |
| `POST /fleet-libraries` accepts `source_kind:"upload"` with **inline** `skill_markdown` + `trigger_markdown`, content-addressed with `ON CONFLICT DO NOTHING`. Applying a crew therefore needs **zero** daemon code — it is a CLI loop over shipped endpoints. | `http/handlers/fleet_bundles/resolve.zig:81` |
| That upload path is **shipped and unreachable**. The dashboard hardcodes `source_kind: SOURCE_KIND_GITHUB` with an `owner/repo` input, and the CLI has no verb for it — so the only way to obtain a fleet row is to install some existing library entry. Hand-setup therefore installs `github-pr-reviewer` and overwrites both markdown files; nothing of the template survives on the investigator or repairer. §4a exposes the shipped path instead. | `AddLibraryDialog.tsx:92,107,141`; `cli/src/program/cli-tree-fleet.ts` |
| **Egress is bounded twice, and the tool-shaped ring is the inner one.** `network/Plan.zig` derives a per-lease egress plan enforced by a network namespace — veth `/30`, static `/etc/hosts` holding only allowlisted names, neutered `/etc/resolv.conf`. It is tool-agnostic, so `git` is egress-bounded exactly as `http_request` is. An earlier reading of `tool_builders.zig` alone concluded `git`/`shell` had "no egress allowlist"; that is true of the tool layer and false of the system. What is genuinely `http_request`-only is **credential substitution** — `secret_substitution` is reachable from `policy_http_request.zig` and nowhere else. | `src/runner/network/Plan.zig:1-12`; `engine/runtime/secret_substitution.zig` call sites |
| The per-lease workspace is `{storage_home}/{lease_id}`, created pre-fork and `deleteTree`d after. Landlock fences it: workspace read-write, `/usr /bin /lib /etc` read-execute, **everything else denied by default** — so a sibling lease's workspace is refused by the kernel, not merely separate. The child cannot supply its own path; the daemon derives it from `lease_id`. Isolation is per-*run*, stronger than per-fleet. | `runner/daemon/lease_run.zig:61-62,210-230`; `runner/engine/landlock.zig:1-9` |
| `MintForwarder` is the shipped pattern for "the child asks the daemon to do something privileged mid-run", and it is lease-bound server-side: *"a child-supplied workspace is impossible — `cp.mint` sends only `lease_id`, and the daemon derives the workspace from it (Invariant 2)."* §4's fetch hook is that pattern with a different payload. Pre-fork materialization (`:181`) was considered and rejected — it fetches on every lease including idle ones, and would have to scrape a repository and commit out of model-authored prose before anything parsed it. | `runner/daemon/lease_run.zig:59-82`, `:181` |
| **Waking a fleet and reconfiguring a fleet are the same permission.** `.workspace_fleet_messages` and the PATCH arm of `.patch_workspace_fleet` both map to `FLEET_WRITE`, and `patch.zig` accepts `trigger_markdown` / `config_json` — where `gates` lives. Any credential able to send the wake message can therefore blank the repairer's gate policy, after which `approval_gate.zig:96` auto-approves and no human is asked. §1's removal of `approval_resolve` does not close this: the bypass requests no approval at all. No narrowing of *which* tenant scopes are granted closes it either — the capability needed and the capability that breaks the design are one capability. Closing it needs `fleet:message` split out of `fleet:write` **and** `agt_a` promoted to a principal (M157_002). | `http/route_scopes.zig:148-153`, `http/handlers/fleets/patch.zig:4-10`, `fleet_runtime/approval_gate.zig:96` |
| A signed webhook cannot substitute for the wake: the manual-route middleware verifies an **HMAC over the body** (`PROVIDER_REGISTRY`), and a model holding `http_request` cannot sign — `${secrets.NAME.FIELD}` substitutes a literal. | `fleet_runtime/webhook_verify.zig`; `docs/AUTH.md` §Manual-route provider scheme registry |
| **A parked approval blocks its fleet's whole queue.** `assign.zig` reads the consumer's own Pending Entries List *first* and refuses to promote newer entries past it — *"promoting a new entry over a possibly-pending gate re-poll would break own-PEL-first ordering."* With the 24-hour default gate timeout, one unattended approval stalls every later event for that fleet for a day, ordered by arrival rather than severity. Serialization itself is correct for repairs (two concurrent repairers would race on the same head); the default timeout is what turns it into a hazard. | `fleet/assign.zig:213-232`; `fleet_runtime/approval_gate.zig` `ActionDetail.timeout_ms` |
| The runtime **cannot** halt a model's loop — *"NullClaw doesn't expose mid-loop interrupt, so we observe instead of force."* It logs a context-fill threshold crossing and `SKILL.md` prose owns the snapshot-and-continue decision. Continuation is therefore a bundle authoring obligation, not a runtime guarantee, and a bundle silent on it runs until it blows the window. | `runner/engine/runner_progress.zig:235-250` |
| Per-lease workspace is `{RUNNER_STORAGE_HOME}/{lease_id}`, default `/tmp/agentsfleet-runner`. Bundle support files extract to its root (`SKILL.md`/`TRIGGER.md` deliberately are not — the lease's own instructions are authoritative), so a clone belongs in a `repo/` subdirectory. The default `/tmp` is tmpfs on many hosts; production sets real disk. | `runner/daemon/config.zig:67,127,129`; `runner/bundle_extract.zig:20-27` |
| Measured on a live fleet: `agentsfleet#586` produced **5 events → 5 leases** for one Pull Request (`synchronize`, `edited`×3, `closed`). The webhook trigger's `events` filter was null, and *"null means fire on every event"* — so a description edit and a close each bought a full model run. | `agentsfleet` dashboard, Aug 03 2026; `fleet_runtime/config_types.zig:104-106` |
| Only Slack, Jira, GitHub (plus Linear, Zoho) are connectors. **Grafana, Elastic, and Fly are plain workspace secrets** — no `api_key` archetype (dropped M108_002). Onboarding is two shapes. | `docs/architecture/connectors.md` §Archetypes |
| `agentsfleet connector` is read-only (`list`, `status`); every *connect* is a dashboard action. `agentsfleet fleet update <id> --from <path>` rewrites a live fleet's markdown — that is the hand-setup path. | `cli/src/commands/connector.ts:1`, `cli/src/program/cli-tree-fleet.ts:37-46` |
| `chain` is documented as a trigger type but rejected by the parser; `delegate`/`spawn` are registered but built inert. Trigger types are `webhook`, `cron`, `api`. | `capabilities.md:44,55` vs `fleet_runtime/config_types.zig:87` |
| The approvals suite's `cleanupTestData` cannot delete the rows it targets: `api_runtime` holds `arw` — not `d` — on `core.fleet_approval_gates`, so the DELETE fails and is swallowed by its `catch` into an `ignored: PG` warning. No test noticed because every other one uses a distinct gate id, and each integration target drops schemas first to compensate. A test that resolves a gate and then needs it unresolved must therefore key on a per-run id, not rely on cleanup. | `\dp core.fleet_approval_gates`; `handlers/approvals/inbox_integration_test.zig` cleanup; `make/test-integration.mk:186-204` |
| `make test-integration` sets `TEST_DATABASE_URL` but **no Redis variables** — those belong to the separate `test-integration-redis` target. Any test guarding on `tryConnectRedis()` therefore *skips silently* under the target most people run, and a green "All integration tests passed" says nothing about it. Confirm a Redis-dependent test by its own pass/skip count, never by the target's exit. | `make/test-integration.mk:225-231` vs `:234-251` |

**Corrections recorded against this session's own output:**

- The reconciling agent initially reported M154 as *blocking* the credential work because
  `core.api_keys` is rebuilt on that branch. Indy pushed back and was right: a renamed
  schema file is a loud git conflict, not a blocker, and the design that landed needs **no
  schema change at all**, so the overlap is a single line in `auth/scopes.zig`.
- The same agent proposed routing the crew handoff through Slack and an egress
  method/path allowlist. Both were withdrawn under adversarial review — see Decomposition
  alternatives (b) and (c).
- Indy's Aug 02 acceptance of "a fleet holding write credentials" was given without anyone
  knowing the minted token covered **every repository in the installation**. The acceptance
  stands; §2 shrinks the surface it was granted over.

### Aug 03, 2026: 03:14 PM — decisions taken

  - > Indy (2026-08-03): "Crew supersedes §1/§2" — context: the multi-fleet shape replaces the single-fleet deterministic apply; the proposal/hash/bounds kernel is deleted in this diff.
  - > Indy (2026-08-03): "Keep it" — context: the detection benchmark stays, against Codex's recommendation to cut it with the pivot. Its rubric row is rewritten to claim detection only.
  - > Indy (2026-08-03): "I dont see a reason for a blocker with M154, why is it a blocker" — context: the M154 escalation was withdrawn; the final design touches no schema.
  - > Indy (2026-08-03): "I dont want to invent too many and get stuck, so i wanna use what we have built with few tweaks, can we not ask the agent to ask for approval to send a PR?" — context: the design pivot to gating the repairer's incoming event rather than building any new approval mechanism. This is the origin of §1–§3.
  - > Indy (2026-08-03): "can there be an auto approval or dangerouslyAccept.. type of as we;;" — context: confirmed already present — `.auto_approve` is the fallthrough when no gate rule matches, so unattended operation is the default and the gate is the opt-in. Dimension 3.2 guards the footgun.
  - > Indy (2026-08-03): "yes" — context: authorising this reconciliation, and with it the deferral of fleet identity and provenance work to M157_002.

### Aug 04, 2026 design session — the lease lifecycle, host cost, and one crew-breaking contradiction

Indy pressed on the lease design: whether a cold start per lease can carry a repairer, whether
the repository is cloned every time, and — narrowing — *"I am more concerned on the clones in
the host"*. Investigation only; no rebuild was warranted.

**Verdict: leases are not rebuilt.** The lease is already four correctly-separated concepts —
the per-fleet slot (`runner_affinity`, the mutex + fencing source), the per-delivery lease row,
the per-run workspace, and per-fleet memory. Any container spanning leases must own a workspace
spanning leases, which forfeits the property the whole design rests on: the daemon derives
`{storage_home}/{lease_id}` server-side, the child cannot name it, and Landlock refuses a
sibling's tree at the kernel. A superset would buy conversational convenience and pay in
isolation.

| Finding | Evidence |
|---|---|
| **A parked gate costs no lease.** An action requiring approval parks the event and *"the lease answers no-work"*; every later poll re-evaluates the recorded gate ref. So a repairer can wait 24 hours on a human at zero lease, zero workspace, and zero clone. This is why gate-bound repair is affordable. | `fleet/approval_gate.zig:4-5,31-37` |
| **`core.fleet_sessions.context_json` is dead.** `claimFleet` reads it and stores it on `FleetSession`; `issueLease` places `instructions` and `bundle_content_hash` on the payload and never the context, and `LeasePayload` has no field for it. It is written faithfully at every report and read into a variable that is freed. The schema comment describes the deleted worker — `fleet_session.zig:5-8` says so outright. **Every lease is a cold start.** | `fleet_session.zig:104,121`; `fleet/service.zig:158-170`; `lib/contract/protocol.zig:253-277` |
| **Continuation is scaffolded and has no producer.** `.continuation` is a first-class `EventType`, `continuationActor` computes a flat non-nesting actor, and `event_rows.zig` lifts `original_event_id` onto `resumes_event_id` — but `continuationActor` has **zero callers** and the report path states *"continuation is a no-op on the happy path (`exit_ok`)"*. A fleet ending with `content='needs continuation'` is XACK'd and nothing re-enqueues it. Dimension 4.8 was unsatisfiable as originally written; it now asserts honest degradation. | `event_envelope.zig:33,88`; `event_rows.zig:84-95`; `service_report.zig:5-6` |
| **Reclaim mints a fresh lease id, therefore a fresh workspace.** `fromReclaim` carries the prior event envelope and the metering cursor forward, but the lease id is new — so a clone, a partial revert, or a pushed branch from the dead holder is invisible to the re-leased run. Work does not survive reclaim; only metering does. | `fleet/assign.zig:303-315`; `lease_run.zig:103-107` |
| **No startup sweep of the storage home, and the default is tmpfs.** Cleanup is `defer cleanupWorkspace`, which does not run on `SIGKILL`, an out-of-memory kill, a panic, or a reboot; startup only `mkdir`s. Today that orphans ≤256 KiB of bundle support files. Once a workspace holds a repository the same shutdown orphans it permanently with no collector. `RUNNER_STORAGE_HOME` defaults to `/tmp/agentsfleet-runner`, and `/tmp` is tmpfs on most hosts — memory, not disk. §4 adds the sweep; §7 pins real disk. | `lease_run.zig:107,227-229`; `runner/main.zig:91`; `runner/daemon/config.zig:129` |
| **A cross-lease repository cache was rejected.** `bundle_extract.zig` is the shipped precedent for a content-addressed cache surviving the per-lease `deleteTree` — and it has **no eviction**, staying small only because every entry is capped at 4 MiB and keyed by an immutable hash. A repository cache inherits the no-eviction property with none of the per-entry bound, turning a bounded always-deleted per-lease cost into an unbounded never-deleted host cost. Bound the fetch instead; keep the delete. | `bundle_extract.zig:22,45-47`, `MAX_BUNDLE_TAR_BYTES` |
| **An omitted `tools:` list yields the full default set.** `buildTools` falls back to `hosted_tools.buildDefault` when `tools` is absent *or* not an array, and that is `allTools` filtered only against the seven cron/schedule names. So a bundle silent on tools gets `shell`, `file_write`, `delegate`, `spawn`, and the memory family rather than its intended surface — the same footgun shape as an omitted `gates.rules` falling through to `.auto_approve`. Dimension 4.10 guards it. | `runner_helpers.zig:242-243`; `tool_bridge.zig:40-48,85-87`; `hosted_tools.zig:17-27` |
| **The investigator cannot do its job without GitHub reads — the original §3 design broke the crew.** `incident-responder/SKILL.md:53-59,89,115` reads `GET /repos/{owner}/{repo}/commits`, correlates deploy annotations against commit history, and verifies `base_sha` against the branch head. Dropping the `github` credential and the `api.github.com` allowlist entry — the prior Files Changed instruction — leaves it no means to name the suspect commit that Dimension 4.2 requires and the repairer's message carries. Resolved by scoping the **mint** rather than removing access: `repository_access: read` mints `contents: read` with no `pull_requests`. §3's structural argument survives and strengthens — the investigator cannot write because its token cannot, not because its prompt says not to. | `library/incident-responder/SKILL.md:53-59,89,115`; `TRIGGER.md` network allow |
| **The broker's token cache would defeat the mint narrowing if the binding stayed out of its key.** `mint` keys the cache on `workspace + integration + identityFingerprint(handle)` — nothing fleet-scoped. Two fleets in ONE workspace minting `github` from the same installation handle therefore collide on one key, and whichever mints first decides the permissions the second receives. That is precisely this milestone's topology: investigator (`repository_access: read`) and repairer (`write`) live in the same workspace and both declare `github`. §2.3 must fold the repository binding and access level into `writeKey`, or a read-only investigator can be handed the repairer's write token from cache. Not visible from the six-file chain alone — found while tracing `broker.mint` for the thread-through. | `credentials/broker.zig:117-136`, `writeKey` |
| **`github-pr-reviewer` declares no repository binding at all**, not even the webhook-ingress one — `repositories` exists only inside the webhook trigger variant (`config_types.zig:107-119`), and that bundle omits it. A fail-closed mint therefore stops the fleet installed from it. Indy's call: fail closed anyway, and skip the operational PATCH because M154's rebuild tears the database down, taking the installed fleet row with it (pre-v2.0 teardown convention). The fixture gains the binding in the same commit so future installs carry it. | `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md`; `fleet_runtime/config_types.zig:107-119` |
| **Prompt caching is priced and metered but never requested.** `cached_input_nanos_per_mtok` exists in the rate tables, `metered_cached_tokens` on the lease row, and `cached_input_tokens` on every report — yet `cache_control` appears nowhere in `src/` **or** the vendored NullClaw engine. On a provider requiring explicit cache blocks, hits are structurally zero and the discounted rate column is dead arithmetic. The investigator's 15-minute sweep is 96 leases/day, each re-paying full input price for its whole `SKILL.md` against a `daily_dollars: 2.00` budget. Re-sending instructions per lease is *correct* — it is how a fleet PATCH takes effect (`bundle_extract.zig:11-15`) — so the fix is a cache-marked stable prefix, never a runner-side cache. Out of scope here; needs its own spec, and needs the request-assembly order verified first. | `state/model_rate_cache.zig:84`; `state/model_library_store.zig:26`; `schema/018_fleet_runner_leases.sql`; absence across `src/` + `zig-pkg/nullclaw-*/src/` |

  - > Indy (2026-08-04): "I dont understand why the repairer will skip writing to memory ... Indy will fix the security holes later. Indy wants the crew to work first" — context: a proposed Dimension forbidding the repairer the memory family was withdrawn. Memory is instead put to work: Dimension 4.11 has the repairer remember what it already opened so a replayed intent yields no second Pull Request.
  - > Indy (2026-08-04): "xgo" — context: authorising the `repository_access` mint split, the storage-home sweep, and the Dimension 4.8/4.9/4.10 corrections recorded above.

### Aug 04, 2026 adversarial review — what §1/§2 actually proved, and what they did not

Codex CLI 0.146.0 (`gpt-5.6-sol`, reasoning effort high, 36 tool calls) reviewed the design
and the landed `origin/main...HEAD` diff adversarially; every finding below was then
re-verified against the code before being accepted. Indy: "Okay go" — authorising the fixes.

**Fixed in this diff.**

| Finding | Evidence | Fix |
|---|---|---|
| **A mid-flight PATCH released events already parked awaiting a human.** `checkApprovalGate` read policy FIRST: dropping `gates` returned `.passed` at the top, and emptying `gates.rules` fell through to `.auto_approve` — either way the parked event executed while its approval card still sat unanswered in Slack. Dimension 3.6 recorded the `fleet:write` bypass as prospective ("no human is ever asked"); it was also RETROSPECTIVE, silently withdrawing a question already asked. Closing the scope split is still M157_002, but honouring a raised gate never needed it. | `fleet/approval_gate.zig:52` (was), `:80-82`; `route_scopes.zig:148-153`; `fleet_runtime/approval_gate.zig:96` | The recorded gate ref is read before any policy read, and outranks every policy outcome. The order is a pure function (`fleet/approval_gate_route.zig`) so it is pinned by unit tests, not by a live Redis. |
| **A parked event incremented the runaway-loop counter on every poll.** `checkAnomaly` is a Redis `INCR` and ran before the re-encounter check, so one human taking their time counted as N runaway attempts — a fleet could be auto-killed for a slow approver. Found while reordering the above. | `fleet_runtime/approval_gate_anomaly.zig:17-46`; `fleet/approval_gate.zig:54-61` (was) | The anomaly check is now reached only on a FIRST encounter. |
| **Machine wakes were attributed to a human.** `tenant_api_key.zig` sets `principal.user_id` from the key's `created_by`, and `buildSteerActor` tested only whether `user_id` was present — so an `agt_t`-driven wake logged as `steer:<human-id>`. §3's "a human wakes the repairer" was therefore unauditable, and an actor-shaped assertion would have certified it while automation did the waking. | `auth/middleware/tenant_api_key.zig:110-115`; `http/handlers/fleets/messages.zig:188-192` (was) | Branch on `principal.mode`. Machines collapse to `steer:api` — the category that function's own doc comment already claimed they got. WHICH key stays unrecorded on purpose: per-key provenance is M157_002, and naming no one is honest where naming the wrong person is not. |
| **The broker cache alias returned, one layer above the mint.** `bindingFingerprint` joined repositories on `KEY_SEP` without length framing, so `["acme/a","acme/b"]` and the single entry `"acme/a<KEY_SEP>acme/b"` hashed IDENTICALLY — a deterministic alias needing no probabilistic collision. Nothing validates repository strings before that point, so the spelling is authorable, and the cache is consulted before the mint that would have refused it. This is the same cross-fleet token bleed §2.3's cache-key work exists to stop. | `credentials/broker_key.zig:52-60` (was); `fleet_runtime/config_repositories.zig:37-50` | Framed with `hashFramed` plus an explicit count — the discipline `hashValue`'s array arm in the same file already used — and seeded with the broker's per-process `fp_seed`, so no digest can be precomputed offline either. |
| **Model prose could counterfeit the card's trusted half, and its evidence was never shown.** `proposed_action` reached the Slack `mrkdwn` block with only JSON escaping, and JSON `\n` renders back as a real line break — so 512 bytes was ample to append convincing `- Gate:` and `- If approved:` rows below the genuine ones. Remaining C0 bytes were not escaped at all, which is invalid JSON (RFC 8259 §7) and would make Slack drop the whole notification for a gate that nonetheless parked. `evidence_json` was never referenced by the Slack builder. | `fleet/approval_gate_detail.zig:89-92` (was); `fleet_runtime/approval_gate_slack.zig` | C0, DEL, and bidirectional overrides are replaced with a space before the prose reaches a card (`fleet/approval_gate_prose.zig`); evidence is rendered in a code span, after the attributed claim. |
| **The card's daemon-vouched half named no repository and no commit.** `tool`/`action`/`params_summary` carry event type, actor, and event id, so every decision-relevant word a human read came from the model. `ActionDetail` gains the fleet's `repository_binding` — the SAME value the GitHub mint scopes the token by, so the reach is statable as fact even though the sha is not. | `fleet/approval_gate_detail.zig:78-80`; `credentials/integration_github.zig` `buildTokenRequestBody` | The card carries `- Token reaches: \`owner/repo\` (write)` as a daemon fact, ahead of the model's claim. |
| **Rubric R7 was failing.** The repair kernel, its tests, `UZ-REPAIR-001..005`, the `REPAIR` error category, and both `tests.zig` registrations were all still on disk despite the Files Changed table marking them DELETE. | `rg 'repair_proposal\|repair_bounds\|UZ-REPAIR' src/` | Swept; R7 now returns zero hits. |
| **Dimension 4.11's test row contradicted its own Dimension.** The row demanded `test_repairer_declares_no_memory_tool` (zero memory tools) while the Dimension, the bundle prose, and Indy's own reversal all require the memory family. §4 is next, so an implementer working from the test table would have rebuilt the thing Indy overruled. | this spec, Dimensions §4 vs Test Specification vs the Aug 04 quote | Row replaced. It also now records that memory is the courtesy and the GATE is the bound — a crash between the vendor call and `memory_store` loses the record, and the second attempt is stopped by needing a second approval. |

**Verified, NOT fixed — carried as known and bounded.**

| Finding | Evidence | Why it is not closed here |
|---|---|---|
| **The Slack approval webhook is a second resolve path that never runs `requireScope`.** `.approval_webhook` sits in the no-auth set and resolves with `.by = SLACK_WEBHOOK`; §1's grant split binds the API path only. Its guard is a single platform-wide `approval_signing_secret`. | `http/route_scopes.zig:86`; `http/handlers/webhooks/approval.zig:52-76`; `cmd/serve_secrets.zig:17` | The secret is boot-resolved daemon config, NOT a workspace secret, so a fleet holding `secret_read` cannot reach it — the tenant-credential attack fails. Invariant 1 stands as written for machine credentials. |
| **No Slack approval records WHICH human approved.** The payload parses `{action_id, decision}` with `ignore_unknown_fields`, so Slack's `user` is discarded and every Slack-resolved gate is attributed to the constant `slack:webhook`. For a milestone headlining "exactly one human approval", the audit answer to *who approved this revert* is a constant string. | `http/handlers/webhooks/approval.zig:124-166`; `fleet_runtime/approval_gate_resolver.zig:13` | Binding a verified Slack user to an `agentsfleet` principal is identity work, and belongs with M157_002's provenance split rather than bolted onto the webhook. Recorded so it is a known gap rather than an assumed property. |
| **Redis is authoritative for approvals.** `readDecisionSourced` returns the Redis mirror first and `evaluateRef` acts on it with no durable-row cross-check, so a writer of `fleet:gate:response:{action_id}` releases the run while the database and inbox still read pending. | `fleet_runtime/approval_gate_async.zig` `readDecisionSourced`, `evaluateRef` | Requires the queue Redis credential — infrastructure-level, and a sandboxed fleet never holds one. Defense-in-depth, not a tenant-reachable bypass, so it is recorded rather than rebuilt mid-workstream. |
| **The mint discards the declared repository OWNER.** `bareRepositoryName` reduces `owner/repo` to `repo` because GitHub scopes installation tokens by name; nothing checks the declared owner against the installation's account, so `otherorg/payments` silently scopes to `<installed-org>/payments` when such a repository exists. | `credentials/integration_github.zig:157-160` | An installation belongs to one account, so this cannot cross a tenant boundary — it mis-scopes within the operator's own installation. §4's fetch hook must validate the FULL `owner/repo` or the two rings disagree; captured here so that lands with the fetch rather than as a separate correction. |

**Design verdict.** The two-fleet split, the pre-lease gate, and the mint narrowing are the
right shape and survived review. The structural gap is that an action-level promise
("approved this exact revert") rests on a run-level approval, and no card fix closes it —
what closes it is either a typed repair intent the daemon enforces at fetch, push, and PR
creation (M157_002-sized), or a Goal sentence that claims what is true: a human released a
run whose credential reaches only the declared repositories. The `- Token reaches:` line
above is that claim made legible.

### Aug 04, 2026 — building §4: the binding the fetch must check does not reach the runner

Found while sizing the fetch hook, after Dimension 4.12 landed.

| Finding | Evidence |
|---|---|
| **The `repositories` binding is invisible to the runner.** It exists only control-plane-side, read by the mint (`credentials/integration_github.zig`) — `src/lib/contract/` carries no repository field at all, and no runner file mentions one. So §4's *"the daemon validates the repository against the fleet's `repositories` binding and refuses anything outside it"* has nothing to validate against today, and Dimension 4.6a's pre-network refusal is unbuildable as written. The scoped token is NOT a substitute: the review already recorded that the mint discards the declared OWNER (`bareRepositoryName`), so a token minted for `otherorg/payments` reaches `<installed-org>/payments` — the vendor ring cannot tell the two apart, which is exactly why the local ring has to. | `grep -rn 'repositor' src/lib/contract/ src/runner/` → zero hits before this workstream; `credentials/integration_github.zig:157-160` |
| **The fix is the shipped pattern, not a protocol break.** `ExecutionPolicy` already carries four additive-and-defaulted fields (`mintable`, `provider`, `inference_host`, `base_url`), each documented as parseable both ways against in-flight leases. `repositories` + `repository_access` ride the policy for the same reason `mintable` does — they describe the grant the child may draw on — and `network_policy.allow` is the direct precedent for a per-lease boundary the runner enforces from the policy. No version bump, no migration, and a lease serialized before the field deserializes to an empty binding, which fails closed. | `lib/contract/execution_policy.zig:97-137` |

**Files Changed additions this implies** (recorded here so the R4 sweep stays honest): `src/lib/contract/execution_policy.zig` EDIT — the two additive fields; `src/agentsfleetd/fleet/service.zig` EDIT — `resolveExecutionPolicy` populates them from the fleet config the mint already reads.

### Aug 04, 2026 — building §4: the fetch execution half

Built after the refusal surface landed. Every constraint below was built against rather than
discovered; what follows is what building them settled.

| Finding | Evidence |
|---|---|
| **The timeout is enforced by polling the child's stderr, not by a timer thread.** `dispatch/write_zig.md` bans a poll loop *around* `child.wait()` because the check lands after a blocking call and is dead code, and prescribes a timer thread. A timer thread is the wrong remedy HERE: `Child.kill` and `Child.wait` both mutate `child.id`, so a killer thread racing the waiting thread reintroduces exactly the kill-after-reap / pid-reuse hazard `child_process.killChild`'s ownership comment exists to prevent. The shipped in-repo answer is used instead — poll under an absolute deadline, kill on breach, single `wait` after — which is `child_supervisor.supervise`'s own ordering (`readResult` → `killChild` → `wait`). The rule's REASON is fully honoured: every bound is checked before the blocking call, never after. | `dispatch/write_zig.md` §Memory Safety Rules; `child_supervisor.zig:169-188`; `child_process.zig:101-106`; `std.process.Child.{kill,wait}` (both null `id`) |
| **stderr is the exit signal, which is why the trailing `wait` is bounded in fact.** Every process holding that descriptor — git and the transport helpers it execs — must be gone before it reads EOF. A read error on it is therefore treated as a LOST bound (kill) rather than as EOF, because the alternative is entering an unbounded wait on the strength of a failed syscall. | `repo_fetch_bounds.watch`; `pipe_proto.waitReadable` |
| **The credential cannot ride argv.** `/proc/PID/cmdline` is world-readable and `/proc/PID/environ` is owner-only, so the installation token goes in the git process's environment as a URL-scoped `http.<url>.extraheader`. Fetching by URL (`git fetch <url> <refspec>`) rather than adding a remote means the token also never reaches `.git/config`, and the tree the child inherits carries the URL at most — which turns Invariant 9 into a grep of the fetched tree rather than an assurance. The host's own git configuration is excluded (`GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` → `/dev/null`) so an operator's credential helper or `insteadOf` rewrite cannot redirect a fleet's fetch. | `repo_fetch_exec.buildEnviron`; `test_fetch_is_bounded_and_credential_free` |
| **git becomes a hard runtime dependency of the runner HOST.** There is no API that computes a three-way merge, so the revert rung needs the binary; `gh`/`glab` would not remove the dependency because `gh repo clone` shells out to git as well. It is resolved from an absolute-path allowlist (`sandbox_args.bwrapPath`'s shape) rather than `$PATH`, and its absence is the named refusal `git_unavailable` rather than a crash. **§7's playbook must install it.** | `repo_fetch_exec.gitPath`; `sandbox_args.bwrapPath:188-194` |
| **Depth 2 over two tips is `{C, C^} ∪ {H, H^}`, and the test measures the cut rather than the flag.** A six-commit fixture yields three commits on the fetched head's ancestry. The revert the repairer would run is then executed against the result — the only assertion that proves `C^` actually arrived, and therefore the only one that justifies depth 2 over depth 1. | `repo_fetch_exec_test.zig`, `EXPECTED_HEAD_COMMITS` |
| **A fetch target that already exists is refused, not reused.** Landlock lets the child create anything at `{workspace}/repo` before the hook runs, and `mkdir(2)` returns EEXIST for a directory, a file, AND a dangling symlink — so create-exclusive is the whole claim, and a squatting child loses its own fetch rather than redirecting it. It also rules out a retried fetch inheriting a half-built tree, which is the "partial tree left for a later step to misread" the Failure Modes table rules out. | `RepoFetchTarget.claim`; `repo_fetch_target_test.zig` |

**Files Changed additions this implies** (recorded so the R4 sweep stays honest): the six
`src/runner/{RepoFetchTarget,repo_fetch_bounds,repo_fetch_exec}.zig` + their test siblings,
all now rows in the table above.

**The transport landed in the same session.** `FetchForwarder` in `lease_run.zig` is where the
two rings finally meet, and the ORDER inside it is the design: `repo_fetch.decide` is pure and
runs FIRST, so an out-of-binding ask is refused before a token is minted and before any packet
— a refused ask costs no credential and no vendor call. The approved ask then carries the
BINDING's spelling forward into the remote URL, the workspace path, and the log, so none of
the three can be steered by how the model capitalized its request. The fetch's own budget is
`min(now + WALL_BUDGET_MS, lease_expires_at)`, so it can never outlive the run that asked.

**Still unbuilt.** The child-side `repo_fetch` tool — one NullClaw tool file modelled on
`runtime/policy_http_request.zig`, one builder, one `BRIDGE_REGISTRY` row, and the channel
wired into `BuildCtx` in `child_exec.zig`. Until that lands nothing in the sandbox can raise a
`repo_fetch_request`, so Dimensions 4.6 and 4.6a stay open: every layer beneath is built and
tested, but DONE means reachable from a running fleet.

**§4 constraints adopted from the review, to build against rather than discover.** The
planned fetch hook runs AFTER the child starts, and Landlock permits the child to create
symlinks anywhere inside its own workspace — so `{workspace}/repo` must be a daemon-owned
directory the child cannot replace, resolved beneath-only with symlinks refused, not a path
the daemon opens by name. `RUNNER_STORAGE_HOME` is uncanonicalized with no sentinel and no
exclusive lock, so "delete every non-dot entry" is only safe behind a canonical path check,
a sentinel file, a held process lock, and strict lease-name validation — otherwise a stray
value or a second daemon during a rolling deploy reaps live work or host data. And a
three-commit depth bounds HISTORY, not BYTES: one commit can carry arbitrarily large blobs,
`disk_write_limit_mb` has no enforcement in `CgroupScope`, and the daemon-side fetch runs
outside the child's cgroup, so `worker_count × one bounded fetch` needs a real byte quota
behind it.

### Aug 04, 2026 — gstack `/review`: the fetch path did not work, and no test could tell

Six independent passes over the branch (testing, security, maintainability, api-contract,
Claude adversarial, Codex adversarial). Four of them converged, without seeing each other, on
one defect: **`repo_fetch` stopped working thirty seconds into any run.**

`FetchForwarder` clamped each fetch's deadline to `self.lease_expires_at`, a snapshot taken
once from `payload.lease_expires_at` and never written again — renewal advances
`RenewDriver.deadline_ms`, a different field. `LEASE_TTL_MS` is 30_000, so
`@min(now + WALL_BUDGET_MS, <a timestamp already past>)` produced an elapsed deadline and
`repo_fetch_bounds.watch` returned `.timed_out` on its first poll, *after* a token had been
minted. The advertised 180 s budget was unreachable by construction. Every test used
`futureDeadline()`, and `FetchForwarder.onFetch` had no test at all, which is why the whole
suite was green over a feature that could not run.

**Fixing the deadline alone would have made things worse**, which is why the two findings are
one change. The fetch is serviced ON the supervisor read loop, and that loop is the only
driver of lease renewal (`applyTick` fires between frames). Letting a fetch have its full
180 s against a 30 s lease TTL guarantees the lease lapses mid-fetch and the control plane
hands the event to a second runner — one approval, two runs. So the fetch now carries a
`RenewTick`: the read loop owns the renew hook and the live usage snapshot, and passes a pump
down into the fetch, which drives it on the quota-check cadence it was already waking on. No
timer thread — `Child.kill` and `Child.wait` both null `child.id`, and that ordering stands.

A third defect fell out of the same loop. **Both bounds were only ever evaluated in the
poll's `.timed_out` arm**, so a child that kept stderr readable held the loop in `.readable`
forever and neither the deadline nor the byte quota was checked at all. git talks to stderr
throughout a fetch, which is exactly that shape. Bounds and renewal now run on a wall-clock
cadence independent of which arm fired, and a completed step is measured once more before it
is certified — `git checkout` materializes the working tree after the last tick.

Three regression tests cover it: the tick fires while a child runs, a tick reporting the lease
lost stops the run, and a child writing steadily to stderr cannot outrun its own deadline.
The last would have hung forever against the old loop.

**Also corrected.** The shipped investigator could not authenticate to GitHub at all:
`SKILL.md` asked for `${secrets.github.api_token}`, but a mintable credential answers only
`.token` (`secret_substitution.zig:117` returns `MissingField` for anything else), so every
GitHub call failed before dispatch. §6's five Dimensions were pinned by tests **no lane ran** —
`bench-incident-test` sat behind `-Dwith-bench-tools=true`, reachable only through
`make bench-incident`, which no workflow calls; nothing in `bench/incident-response/` imports
zBench, so the gate was never load-bearing and the step now runs in `test-unit-agentsfleet-lib`.
§2's four Dimensions named tests that did not exist under those names; every DONE Dimension's
test identifier is now greppable, which is how this repo's spec discipline is meant to be
checked. Invariant 9 still described the fetch as running *pre-fork* — the design §4 spends
three paragraphs rejecting — and the Invariants block was numbered out of order.

**Left open, deliberately, for Indy.** Two findings are architecture calls rather than defects:
(a) the daemon runs `git` inside the child's read-write workspace with only
`GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` neutralised, and repo-local `.git/config` cannot be
disabled — whether that is reachable turns on whether a child-backgrounded process survives
across tool calls, which was not established; and (b) the mint strips the owner from
`owner/repo`, which `repo_fetch.decide` catches on the fetch path but not on the
`${secrets.github}` path into `http_request`. Both are recorded here rather than fixed.

Not a defect: `route(.unreadable, null) → .pass` is deliberate, documented, and pinned by
`approval_gate_route.zig:87`. The unscoped `approval_webhook` route is real but
`route_scopes.zig` is untouched by this branch — pre-existing, and it qualifies Invariant 1's
wording rather than this workstream's change.

### Aug 05, 2026 — building §3: the gate matches events, not tool calls

Two things about the shipped runtime shaped what the repairer's bundle could say.

**The gate rule's `tool`/`action` fields do not mean what they are named.** The
only production caller is `checkApprovalGate`, which passes
`evaluateGate(gates, event.event_type, event.actor, context)` — so `tool` is
matched against the event TYPE (`chat`/`webhook`/`cron`) and `action` against the
event ACTOR (`steer:<uid>`, `webhook:github`). Every example in the gate tests is
tool-shaped (`{"tool":"git","action":"push"}`, `{"tool":"github","action":"create_pr"}`)
and would therefore match nothing at the only site that evaluates it. An author
following those examples gets a rule that never fires, and `approval_gate.zig:96`
falls through to `.auto_approve` — an autonomous agent holding a write token,
which is precisely what Dimension 3.2 exists to prevent. The repairer declares
`tool: "*", action: "*"`, which is also the semantically correct rule: approval
RELEASES the run rather than permitting one step inside it. **The naming is worth
fixing on its own** — it is a trap for every future bundle author — and
Dimension 3.2's test now asserts the rule MATCHES a real wake rather than merely
parsing, so a tool-shaped regression fails instead of silently auto-approving.

**The investigator's SKILL.md still described the retired repair kernel.** It
instructed the model to emit a ` ```json repair_proposal/1 ` block and claimed
"the platform validates this block, stores it immutably, and asks a human" — but
`51d2c256f` deleted `repair_proposal.zig`, `repair_bounds.zig` and the whole
`UZ-REPAIR-*` family when the crew design superseded that kernel. The model would
have emitted a block nothing reads. Replaced with what the design actually wants:
a prose repair INTENT, plus the explicit statement that the investigator cannot
start the repair itself because it holds no credential that can. Its "one tool"
section was also stale (it now has three), and both bundles gained the named
degradation and the explicit "nothing continues you" that Dimension 4.8 requires.

`incident-responder` also had no `repositories`/`repository_access` at all, so
its own GitHub mint failed closed — the bundle shipped with a `github` credential
it could not use. Both keys are required together; `read` is the boundary the
whole crew rests on, and Dimension 3.1's test drives the SHIPPED binding through
the real mint so raising it to `write` fails the suite.

Not fixed here, and flagged — the coverage lane's timing debt is now live on
this branch. Two unrelated tests failed once and passed on a clean re-run:
`queue/redis_pool_test`'s acquire-timeout locally under parallel load, and
`catalog_etag_integration_test`'s "If-Match check serializes with a concurrent
catalog write" in Continuous Integration (CI) under `test-coverage-zig`
(`CatalogPatchNeverBlocked`). Neither file is touched by this branch; the second
arrived in M131 and polls `pg_stat_activity` for a lock waiter on a bounded
retry, which is exactly the shape kcov's ptrace slowdown breaks. This is the
same debt already recorded as Indy's call: `test-coverage-zig` runs the whole
integration binary under kcov, so every timing-dependent integration test is
evaluated twice under conditions the `test-integration` lane never applies. The
fix remains (b) skip those under the coverage lane or (c) exclude integration
tests from the coverage binaries — never widening an allowlist, which would let
a real hang pass.

### Aug 05, 2026 — the two architecture calls Indy left open

Both were carried unfixed out of the gstack `/review` session. Indy took them
separately: settle (a) by establishing its deciding fact, fix (b).

**(a) The deciding fact is established, and the answer is YES.** A process
backgrounded by a successful `shell` tool call **survives that tool call**.
`process_util.run` sets `child.pgid = 0` to isolate the child's process group,
but `terminateChild` — the only thing that signals that group and walks
`/proc/<pid>/task/<pid>/children` — is reachable **only** from
`processWatcherMain`, and only on the cancel or timeout arms. The success path
sets `done`, joins the watcher without signalling anything, reads both pipes to
EOF, and reaps the direct child alone. A grandchild that redirected its inherited
descriptors closes those pipes immediately, so the tool call returns promptly and
the grandchild is left running inside the sandbox for the rest of the lease.

| Finding | Evidence |
|---|---|
| A backgrounded grandchild outlives its tool call. `terminateChild` signals `-child.id` and recursively kills the `/proc` children, but nothing calls it unless the watcher observed a cancel or a timeout. | `zig-pkg/nullclaw-*/src/tools/process_util.zig:186-205` (watcher arms), `:160-184` (`terminateChild`), `:409-412` (success-path `defer` — sets `done`, joins, never signals), `:436` (`child.wait()` reaps the direct child only) |
| So the window between the daemon's three git spawns is real in principle. `git init`, `git fetch`, and `git checkout` are separate spawns against `{workspace}/repo`, which is bound read-write for the child (`sandbox_args.zig:159`), and repo-local `.git/config` is the one layer `repo_fetch_env.build` cannot switch off — `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` cover the other two and git has no equivalent for the local file. | `repo_fetch_env.zig:47-49`; `repo_fetch_exec` step ordering |
| **The shipped repairer cannot reach it.** Its declared tools are `repo_fetch`, `git`, `http_request`, `memory_store`, `memory_recall`. None can background a process or write an arbitrary file: the `git` tool is restricted to an eight-value `operation` enum (`status`/`diff`/`log`/`branch`/`commit`/`add`/`checkout`/`stash`) with no `config` arm, and it explicitly rejects `-c` config injection. There is no `shell` and no `file_write`. | `library/incident-repairer/TRIGGER.md` `tools:`; `zig-pkg/nullclaw-*/src/tools/git.zig:19` (schema enum), `:73` (`-c` block) |
| **But that safety is bundle-shaped, not design-shaped.** `runner_helpers` falls back to `hosted_tools.buildDefault` when `tools:` is absent *or* not an array, and that set includes `shell` and `file_write`. So the first fleet that declares `repo_fetch` and omits `tools:` gets both the window and the means — and the daemon's git steps carry the minted token in their environment, which is what makes the window worth closing rather than merely noting. | `runner_helpers.zig:242-243`; `hosted_tools.zig:17-27`; Dimension 4.10 |

**Verdict: not reachable today, reachable by omission tomorrow.** Recorded as a
hardening item with a named trigger rather than as a merge blocker.

Two further facts bound it. `sandbox_args.zig:147` passes `--die-with-parent`
and `--unshare-all`, and the latter includes a Process Identifier (PID)
namespace — bwrap is PID 1 inside it, so the kernel reaps every descendant when
the sandbox tears down. **Nothing survives a lease**, so the blast radius was
never cross-lease; the exposure is strictly within one run, which is where the
fetch happens. And the root cause is upstream rather than ours: the vendored
`nullclaw-2026.5.29` is AHEAD of the `~/Projects/oss/nullclaw` checkout
(`2026.4.17`), so patching `zig-pkg/` directly would evaporate on the next
dependency bump. The upstream fix is five lines — signal `-child.id` after both
pipes reach End Of File (EOF) and BEFORE `wait()`, which nulls `child.id` — but
it is also a policy change, since it kills intentional backgrounding too.

Two fixes, deliberately separated:

- **Ours, and the one that makes the property structural:** run the three git
  steps in a daemon-private directory and `rename(2)` the finished tree into
  `{workspace}/repo`. It closes the window regardless of what runs in the
  sandbox — no dependence on nullclaw's process policy and none on a bundle's
  `tools:` list, so "reachable by omission" stops being true rather than being
  guarded. `RepoFetchTarget` claims the scratch path instead of the workspace
  path; both live under the storage home, so the rename is same-filesystem and
  atomic.
- **Upstream, filed separately:** the `process_util.run` sweep.

  - > Indy (2026-08-05): "well keep going i want to test and keeping it. not get bogged down by now" — context: both fixes deferred out of M157_001. The milestone keeps the finding recorded and the shipped crew safe by its declared `tools:` list; neither hardening lands here.

**(b) Fixed: the mint now refuses a token that reaches something else.** The
request body names repositories by BARE name — GitHub scopes an installation
token by name within the installation's own account — so the owner a fleet
declared never reached the wire, and `acme/payments` minted happily against
`<installed-account>/payments`. `repo_fetch.decide` compared the qualified
spelling on the fetch path; the `${secrets.github}` path into `http_request`
compared nothing, so one declaration meant two different things depending on
which path the model took.

The fix checks the RESPONSE rather than re-deriving the request: a
create-installation-access-token response echoes a `repositories` array carrying
the qualified `full_name` of every repository the token reaches, so comparing
that set against the declared set validates what the credential can actually
touch — including a mis-scope whose cause nobody modelled. Set equality in both
directions, case-insensitively (GitHub owners and repository names are), and a
response stating no reach at all is refused rather than read as "all of them" —
which is precisely the pre-narrowing behaviour §2 removed. A rejected token is
never duplicated and never returned; it dies with the parsed document.

`integration_github.zig` crossed RULE FLL on the change, so the binding's two
halves are now named files either side of it: `integration_github_body.zig`
(what the mint asks for) and `integration_github_reach.zig` (whether the token
it got back matches). Six unit tests on the pure verifier plus two driven
through the public `mint`, including the stripped-owner regression itself.

### Aug 05, 2026 — the repairer cannot run the one command it exists to run

Found while building Dimension 4.5's conflict fixture.

| Finding | Evidence |
|---|---|
| **There is no `revert` operation.** `library/incident-repairer/SKILL.md:45` instructs *"Run `git revert --no-edit <commit>`"*, and the fleet's `git` tool is nullclaw's `GitTool` verbatim — `tool_builders.buildGit` constructs it with no wrapper. That tool dispatches through a closed operation map and answers anything outside it with `Unknown operation: <op>`; the set is `status`, `diff`, `log`, `branch`, `commit`, `add`, `checkout`, `stash`. The repairer holds no `shell` either, and that absence is deliberate and asserted. So the fetch path lands a working tree the fleet has no means to act on. | `library/incident-repairer/SKILL.md:45`; `engine/tool_builders.zig:113-117`; `zig-pkg/nullclaw-*/src/tools/git.zig:19,160,171`; `crew_bundle_test.zig` (`!hasTool(repairer, "shell")`) |

Same shape as the `${secrets.github.api_token}` defect the Aug 04 review caught:
a bundle asking the runtime for something it does not have. It blocks
Dimension 4.5 and Dimension 4.4, and with them the milestone's headline claim.

**Indy's call: not now.** The fix is an agentsfleet-side `git_revert` child tool
mirroring `repo_fetch` — one tool file, one builder, one `BRIDGE_REGISTRY` row,
needing no network and no credential since the tree is already in the workspace.
That tool is also where Dimension 4.5's NAMED refusal belongs, so the two land
together or not at all. Adding `revert` to the vendored tool was rejected for
the same reason as the other upstream fix (the vendored tree is ahead of the
local checkout); granting `shell` was rejected outright, since it forfeits "no
model-authored source lines" — the only property that makes revert the safe
first rung — and re-opens the `.git/config` window recorded above.

  - > Indy (2026-08-05): "i think thats an overkill? for now" — context: the `git_revert` child tool deferred out of M157_001. Dimensions 4.4 and 4.5 stay open and are NOT claimed; the milestone ships the gate, the mint, and the fetch, and stops short of the revert executing.
