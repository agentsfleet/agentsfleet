# Handoff — M148_001 assigned runner policy (Jul 30, 2026, 10:42 PM)

Ephemeral — briefs the next agent; CHORE(close) deletes this file before the
Pull Request (PR) opens. The spec is the canonical state:
`docs/v2/active/M148_001_P1_API_INFRA_UI_CONTROL_PLANE_ASSIGNED_RUNNER_POLICY.md`.

## Scope/Status

Inverting runner configuration: the dashboard ASSIGNS policy (tier, network,
registry allowlist, workers); the host applies it, probes what it can enforce,
and the control plane reconciles the two into a visible degraded verdict.

- ✅ CHORE(open) done — spec in `active/`, baseline `unit=3266 integration=501`.
- ✅ §1 policy-down: migration 042, contract payloads (`AssignedPolicy`,
  `CapabilityReport`, `NetworkPolicy` moved to contract), register assigns +
  echoes, self + every heartbeat deliver policy, fleet PATCH takes
  `{assigned_policy}` one-of (idempotent, `runner_policy_assigned` audit
  event), OpenAPI + dashboard client (`createRunner` sends the envelope).
- ✅ §2 runner-obeys: Config = bootstrap trio (`AGENTSFLEET_API_URL`,
  `AGENTSFLEET_RUNNER_TOKEN`, optional `RUNNER_STORAGE_HOME` — renamed from
  `RUNNER_WORKSPACE_BASE`); `AppliedPolicy.zig` holder fed per heartbeat
  (lenient two-stage parse; fixed a latent use-after-free in the old
  pluck-then-free client); per-lease effective config; worker soft-shrink;
  boot gates (release `dev_none` refusal, cgroup enablement) moved to
  policy-arrival in `policy_apply.zig` — refuse-and-heartbeat, no crash loop.
- ✅ §3 probe + reconcile (code complete): `capability_probe.zig` (Landlock
  ABI, seccomp prctl, delegated `subtree_control` read, bwrap presence,
  `egress_enforcement=false` pinned); loop probes per tick, sends
  first/changed, obeys reply verdict (`UZ-EXEC-017`);
  `heartbeat_reconcile.zig` pure verdict + reason vocabulary; heartbeat
  stores report+verdict / re-reconciles stored (steady state writes nothing);
  lease handler issues nothing to a degraded runner (fail closed).
- ⏳ §3 tail: the batched DB-backed integration suite is NOT yet written —
  spec tests 1.1, 1.2, 1.3, 2.1, 3.2, 3.3, 3.5 (one suite: register →
  heartbeat → PATCH → degraded → recovery). Model on
  `src/agentsfleetd/http/handlers/runner/credentials_mint_integration_test.zig`
  + `src/http/test_harness.zig` rules (live test DB, no temp tables, fixtures
  via `test_fixtures_<scope>.zig`, cleanup in body not defer).
- ⏳ §4 dashboard: RunnerList degraded badge + assigned-vs-achievable;
  AddRunnerDialog gains all four policy fields (Indy decision — network
  defaults `allow_all`); NEW scope Indy asked for personally: **"Edit policy"
  action on the runner row** (reuse the four-field form, calls the landed
  PATCH). Server side: `runners_list.zig` + `fleet/sql.zig` must surface
  assigned/achievable/degraded (RunnerSummary/RunnerDetail OpenAPI schemas
  gain fields then; `RunnerCapabilityReport` schema still unadded — add with
  §4). TS types in `ui/packages/app/lib/api/runners.ts` (AssignedPolicy etc.
  already exist).
- ⏳ §5 teardown: playbooks 06/07/onboarding, `deploy/baremetal/deploy.sh`
  required-env list, `agentsfleet-runner.service` comment (also stale
  "allow_all is the current default" claim), `provision_runner_env_test.sh`;
  R5 grep must hit zero outside `docs/v2/`.
- ⏳ §6 seatbelt removal: enum + UI list + OpenAPI + tests + comments; R7 grep
  zero outside `docs/v2/` + `schema/017` comment; `reconcile.tierNeeds` and
  `main.zig`/`policy_apply` test cases reference `.macos_seatbelt` — rework.
- ⏳ VERIFY chain (order is mandatory): `/write-unit-test` →
  `/write-integration-test` → gstack `/review` → docs/changelog (DOCUMENT) →
  CHORE(close) → PR → `kishore-babysit-prs`. Rubric R1–R7/S* graded in-spec.

## Working tree

Clean. Branch `feat/m148-assigned-runner-policy`, worktree
`~/Projects/agentsfleet-m148-assigned-runner-policy`. 10 local commits, none
pushed; no PR yet (PR opens only after CHORE(close); `orly gate pr` must be
green). No tmux sessions or background processes running.

## Tests/checks run

- `make test-unit-agentsfleetd` ✅ 2018 pass / 296 skip
- `make test-unit-agentsfleet-runner` ✅ 381 pass / 7 skip
- `make test-unit-agentsfleet-lib` ✅ 86+30+32 pass
- App vitest ✅ 209 files / 2090 tests; `bun run typecheck` + oxlint ✅
- `make check-openapi` ✅ (bundle regenerated, route coverage green)
- Cross-compiles ✅ x86_64-linux + aarch64-linux every commit; linux test
  graph compiles ("unable to execute binaries" = the PASS signal)
- NOT yet run: `make test-integration`, `make test-e2e`, `make memleak`,
  full `make test` / `lint-all` sweep (VERIFY stage)

## Next steps (in order)

1. Write the batched integration suite (§3 tail) — see model files above.
2. §4 server side (`runners_list.zig`, `fleet/sql.zig`, OpenAPI Summary/
   Detail + `RunnerCapabilityReport`), then UI (RunnerList badge, four-field
   AddRunnerDialog, Edit-policy row action) + UI tests.
3. §5 teardown + R5 sweep; §6 seatbelt removal + R7 sweep.
4. Spec bookkeeping: mark Dimensions DONE as their tests land; update AUTH.md
   register diagram (`{host_id, sandbox_tier}` → `assigned_policy`) and
   `docs/architecture/runner_fleet.md` (inversion + reconciliation loop) at
   DOCUMENT; changelog `<Update>` + docs.agentsfleet.net pages at CHORE(close).
5. VERIFY chain per the order above, grade the rubric, CHORE(close), PR.

## Risks/gotchas

- FLL 350-line cap: `control_plane_client.zig` sits at exactly 350 and
  `sql.zig`/`heartbeat.zig` are growing — next addition to either forces a
  split (client: consider extracting the memory/bundle verbs).
- Pre-commit `make -j` graph once failed on stale formatting — run `zig fmt`
  on touched files before committing; standalone-lane reruns confirmed green.
- The UFS gate flags any string literal repeated twice per file (including
  test fixtures) — name constants as you write, not after.
- Info-level log allow-list is FIXED (`server_started`, …) — new events go
  `debug`, or `warn` with a registered `error_code`.
- Rollout note (already told Indy): pre-migration runner rows have NULL
  `network_policy` → they read degraded "no assigned policy" until an
  operator assigns one via the dashboard/PATCH. Indy explicitly declined the
  env-var unbrick; the dashboard path IS the fix.
- Deferrals with Indy-acked quotes live in the spec's Discovery: the
  `EgressScope` enforcement is a separate follow-up workstream; nothing else
  is deferred — treat any other gap as in-scope.
- Findings artifact for Indy (private):
  https://claude.ai/code/artifact/6fdfb8d1-df0c-4cca-9c9e-4abd2fa29fb7
