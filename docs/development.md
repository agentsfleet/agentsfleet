# Development notes — repo-specific operational knowledge

> Contributor-facing notes for the things `make help` and the architecture set
> don't cover: how the hooks behave, what the test lanes actually mean, and the
> environmental traps that cost real hours. Each section records verified
> behavior with the incident that taught it. When code changes make a section
> stale, fix it in the same PR.

## Pushing and git hooks

Hooks live **in this repo** at `.githooks/` (`git config core.hooksPath=.githooks`)
— not in any contributor's dotfiles. Hook fixes are normal repo PRs.

- **pre-push classifies the outgoing range into unit surfaces** by file pattern
  (`*.zig`/`schema/*.sql` → agentsfleetd, `agentsfleet/*` → agentsfleet, `ui/packages/app/*`
  → app, website, design-system → both, bundle dirs → bundle). Zero matches →
  `"no test-relevant files — nothing to run"` and the push sails through. A
  genuinely docs-only push skips the suite entirely.
- **The merge trap:** merging `origin/main` *into* a branch makes the pushed
  range include all of main's recent source files — pre-push then runs the full
  unit lanes for what was a docs-only intent, and `test-unit-agentsfleetd` **hangs if
  the test-DB containers are down**. The lean-push pattern for docs-only
  branches: don't local-merge main; push just the docs commit (skips), and sync
  the branch with GitHub's **"Update branch"** button (server-side, no local
  hook).
- **Never run two pushes concurrently.** Each pre-push spawns a DB-backed
  agentsfleetd test suite; two at once deadlock on the shared test Postgres at 0% CPU
  forever and block every subsequent push. Recovery: kill the stuck
  `agentsfleetd-tests --listen` / `zig build test` processes and retry serially.
- **Sandboxed agent environments break the SSH transfer.** Hook verification
  passes, then the upload dies with `Broken pipe` / `Connection closed by remote
  host` on every attempt. It is not payload size — run the push with network
  sandboxing disabled and it lands first try.
- **Flaky under parallel hook load:** `agentsfleet test/browser-resolve-platforms`
  (and occasionally app's `provider-selector.test.ts`) time out at ~5 s but pass
  in isolation. Retry serially before suspecting the diff.
- **`main` is branch-protected** (required checks; direct pushes are declined).
  Everything lands via a feature branch + PR — including specs.
- `make test-integration` (real Postgres+Redis) and memleak run **in CI only**;
  pre-push runs only the fast `test-unit-*` lanes.

## Test lanes — what the names mean

- **"Live e2e" / acceptance = the Playwright ladder**, not backend integration:
  `ui/packages/app` → `bun run test:e2e:acceptance` (signup, login, lifecycle,
  kill, billing, multi-workspace). Local twins of the CI jobs:
  `make acceptance-e2e` (app suite; local run auto-starts dev on :3101, needs
  Clerk DEV creds in the worktree-root `.env`) and `make cli-acceptance`
  (agentsfleet). CI runs the same suite against the dev deployment on PR and prod
  post-deploy.
- **`make test-integration` owns the backend Zig suite** against ephemeral
  Postgres+Redis. There is deliberately **no umbrella target** re-aliasing it
  under an acceptance name — a proposal to add one produced a byte-identical
  duplicate target and was removed.
- **Daemon execute-loop without a language model:** build with
  `-Dexecutor-provider-stub` (`build_runner.zig`). The flag is comptime-eliminated
  in production (no env backdoor): `child_exec` emits a canned `result` frame,
  and the integration target forks a prebuilt stub-flagged
  `agentsfleet-runner-execstub` exe per lease. Exercised by
  `src/runner/worker_pool_integration_test.zig` via `make test-integration-runner`.
- **Benign stderr in green runs:** `expected 5, found 1` /
  `expected .worker_started, found .server_started` lines come from the
  *negative* tests in `telemetry_test.zig` (deliberate mismatches asserted via
  `expectError`). They print on every clean `make test-unit-agentsfleetd`. Not a
  failure.
- **Cross-compile proof for the test graph:** on macOS,
  `zig build test -Dtarget=x86_64-linux` reports a RUN-step failure (can't exec
  a Linux ELF) — use `zig build test-bin -Dtarget=...` for a build-only EXIT=0
  proof.

## Linting

- `make lint-zig` runs `zlint --deny-warnings` **bare** — zlint is git-root-aware
  and walks every tracked `.zig` (all of `src/agentsfleetd`, `src/lib`, `src/runner`,
  build files). **Never pass it a path argument**: `zlint src/agentsfleetd` scans
  **0 files and exits 0** — a silent pass, not a scoped run. To lint a subset,
  use stdin mode: `find <dirs> -name '*.zig' | zlint -S`.

## Dead-code auditing (`src/`)

Auditing for dead `.zig` files needs **two reachability walks**, and both must
model Zig's transitive test-block compilation:

- **PROD reach** — breadth-first from the binary entrypoints (`src/agentsfleetd/main.zig`,
  `src/runner/main.zig` via `build_runner.zig`) + the named module roots
  (`log`/`contract`/`common`/…) + `bench_exports.zig`.
- **TEST reach** — breadth-first from the test aggregators only
  (`src/agentsfleetd/tests.zig`, `src/agentsfleetd/auth/tests.zig`, `src/runner/tests.zig`,
  `src/lib/tests.zig`), following **all** `@import`s — in a test build,
  `test {}` blocks compile, so a parent module's
  `test { _ = @import("x_test.zig"); }` pulls the test file in *transitively*.

The trap: grepping "is this `_test.zig` imported by `tests.zig` directly?"
produced **16 false positives** in one sweep — files like
`error_registry_test.zig` run via their parent's test block, never via the
aggregator directly. Non-test files reachable in TEST but not PROD are
production-dead (test-kept); `_test.zig` files in neither walk are true orphans.

## agentsfleet CLI conventions

- **Hidden flags are registered on the root program**, never on a subcommand —
  commander renders any subcommand with options as `cmd [options] …`, which
  widens the auto-computed help column past 80 chars and breaks both the
  byte-exact help golden and the 80-column test. `.hideHelp()` hides the flag
  from `cmd --help` but not the `[options]` term in the parent listing. Accepted
  trade-off: the flag parses as a global no-op on other subcommands.
- **Effect-TS follows the Supabase CLI reference** at
  `~/Projects/oss/cli/apps/cli/src/next/` — service surface, layer composition,
  handler shape, error mapping. Any divergence (even Tag-construction or layer
  order) needs an explicit maintainer ack **before** the diff lands; surface the
  reference shape, the proposed divergence, and the why.

## Deploys (Vercel)

- Token at runtime: `VERCEL_TOKEN=$(op read 'op://ZMB_CD_DEV/vercel-api-token/credential')`
  — never print it. Team scope `indykishs-projects`.
- Projects → domains: `agentsfleet-website` (marketing) · `agentsfleet-app`
  (dashboard) · `usezombie-agents-sh` (serves the `usezombie.sh` installer
  domain; static output `ui/agentsfleet.dev/dist/`).
- **Preview URLs return 401** (`ssoProtection: all_except_custom_domains`); prod
  custom domains are raw-reachable. To curl a preview, fetch the project's
  automation-bypass secret (`GET /v9/projects/{name}` → `.protectionBypass |
  keys[0]`) and send `x-vercel-protection-bypass: <secret>`.
- Vercel ignores Cloudflare-Pages `_redirects`/`_headers`; static dirs need
  `framework=Other` + a `vercel.json` for rewrites/headers. The `usezombie.sh`
  root rewrite + `text/x-shellscript` content-type live in
  `ui/agentsfleet.dev/dist/vercel.json` (two explicit header sources — the
  optional-group regex form does not match bare `/`). Deploys ride the git
  integration: preview-on-PR, prod-on-merge.
- Parsing `/v6/deployments` JSON: use `jq` — python's `json.load` chokes on
  control characters in the response.

## Dashboard performance — read dev numbers carefully

The dev-mode `/zombies` 1.5–5 s is mostly **not** a backend bug: Turbopack
on-demand route compilation (zero in prod) + local dev calling the **remote**
`api-dev` backend (`lib/api/client.ts` `API_ORIGIN` default) + uncompressed dev
RSC streaming. The `route?_rsc=…` request is the App Router navigation payload,
not the JSON API. The Approvals 5 s repeat is an intentional poll; the Clerk
`/touch`+`/tokens` pair is SDK session management doubled by StrictMode in dev.
The one prod-relevant lever (own perf PR): the server components make 3
*sequential* remote hops (`getToken → workspaces → zombies`) — parallelize
billing with workspace resolution and skip the workspaces round-trip when the
`active_workspace_id` cookie is set. Measure a Vercel preview first; never
optimize against dev numbers.

## Synced tooling (not repo-owned)

`scripts/audit-*.sh`, `scripts/lib/`, and `scripts/llmevals/` appear untracked —
they re-sync from the operating-model tooling via `upgrade-ai-tools`. Don't
commit them, don't treat a worktree missing them as data loss, and don't block
worktree cleanup on them.
