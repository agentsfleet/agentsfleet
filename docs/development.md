# Development notes — repo-specific operational knowledge

> Contributor-facing notes for the things `make help` and the architecture set
> don't cover: how the hooks behave, what the test lanes actually mean, and the
> environmental traps that cost real hours. Each section records verified
> behavior with the incident that taught it. When code changes make a section
> stale, fix it in the same PR.

## Pushing and git hooks

Hooks live **in this repo** at `.githooks/` (`git config core.hooksPath=.githooks`)
— not in any contributor's dotfiles. Hook fixes are normal repo PRs.

- **pre-push classifies the outgoing range into lint and unit surfaces** by file
  pattern (`*.rs`/`rustd/*` → `lint-rustd` + `test-unit-rustd`, `cli/*` → cli,
  `ui/packages/app/*` → app, website → website, design-system → both it and
  app). Zero matches → `"no test-relevant files — nothing to run"` and the push
  sails through. A genuinely docs-only push skips the suite entirely.
- **Clippy is pre-push's, not pre-commit's.** `lint-rustd` compiles the whole
  workspace under `-D warnings`, which measured 36s to 3m16s per commit — a tax
  on every commit including the small ones. Pre-commit keeps the checks that
  cost seconds: the partial-staging reject, `orly gate work` (the `conform`
  row — UFS, RUST ERR, LOGGING and the rest, over the staged diff), gitleaks,
  and the fast per-surface gates. This is the split `docs/VERIFY_TIERS.md`
  already describes; the hooks had drifted from it.
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
  (and occasionally the app's provider-selection tests) time out at ~5 s but pass
  in isolation. Retry serially before suspecting the diff.
- **`main` is branch-protected** (required checks; direct pushes are declined).
  Everything lands via a feature branch + PR — including specs.
- Pre-push runs the lanes matching what you pushed: `*.rs` or anything under
  `rustd/` triggers `test-unit-rustd`, and each TypeScript package triggers its own.

## Test lanes — what the names mean

- **"Live e2e" / acceptance = the Playwright ladder**, not backend integration:
  `ui/packages/app` → `bun run test:e2e:acceptance` (signup, login, lifecycle,
  kill, billing, multi-workspace). Local twins of the CI jobs:
  `make acceptance-e2e` (app suite; local run auto-starts dev on :3101, needs
  Clerk DEV creds in the worktree-root `.env`) and `make cli-acceptance`
  (agentsfleet). CI runs the same suite against the dev deployment on PR and prod
  post-deploy.
- **`make test-unit-all` is the repository's unit claim**: the Rust workspace
  (`cargo test --workspace`) plus each TypeScript package's coverage gate. A
  package-scoped runner proves that package, never the repository. There is
  deliberately **no umbrella target** re-aliasing a lane under a second name — a
  proposal to add one produced a byte-identical duplicate target and was removed.
- **Daemon execute-loop without a language model:** build with
  `-Dexecutor-provider-stub` (`build_runner.zig`). The flag is comptime-eliminated
  in production (no env backdoor): `child_exec` emits a canned `result` frame,
  and the integration target forks a prebuilt stub-flagged
  `agentsfleet-runner-execstub` exe per lease. Exercised by
  `src/runner/worker_pool_integration_test.zig`. That lane retired with the Zig
  test lanes; the flag and the stub remain in the build graph.
- **Cross-compile proof for the test graph:** on macOS,
  `zig build test -Dtarget=x86_64-linux` reports a RUN-step failure (can't exec
  a Linux ELF) — use `zig build test-bin -Dtarget=...` for a build-only EXIT=0
  proof.

## Linting

- `make lint-all` is the lint claim. `lint-rustd` runs `cargo fmt --check` plus
  `cargo clippy --workspace --all-targets -- -D warnings`; `lint-scripts` runs
  every `scripts/*_test.py`; the TypeScript, shell and OpenAPI checks follow.
- Both cargo steps `cd` into `rustd/` rather than passing `--manifest-path`:
  `rust-toolchain.toml` resolves from the working directory, so running cargo
  from the repository root compiles with whatever toolchain the shell has active
  instead of the pinned one.

## Dead-code auditing (`src/`)

Auditing for dead `.zig` files needs **two reachability walks**, and both must
model Zig's transitive test-block compilation. The Zig tree is the runner and
the library it links (`src/runner`, `src/lib`); the daemon is Rust under
`rustd/`, and `cargo`'s own dead-code lints cover it.

- **PROD reach** — breadth-first from the binary entrypoint (`src/runner/main.zig`
  via `build_runner.zig`) + the named module roots the runner's `SharedDeps`
  wires (`log`/`contract`/`common`/…).
- **TEST reach** — breadth-first from the test aggregators only
  (`src/runner/tests.zig`, `src/lib/tests.zig`), following **all** `@import`s —
  in a test build, `test {}` blocks compile, so a parent module's
  `test { _ = @import("x_test.zig"); }` pulls the test file in *transitively*.

The trap: grepping "is this `*_test.zig` imported by `tests.zig` directly?"
produced **16 false positives** in one sweep of the old daemon tree — test
files that ran via their parent's test block, never via the aggregator
directly. Non-test files reachable in TEST but not PROD are production-dead
(test-kept); `*_test.zig` files in neither walk are true orphans.

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
  (dashboard) · `agentsfleet-agents-dev` (serves the `agentsfleet.dev` installer
  domain; static output `ui/agentsfleet.dev/dist/`).
- **Preview URLs return 401** (`ssoProtection: all_except_custom_domains`); prod
  custom domains are raw-reachable. To curl a preview, fetch the project's
  automation-bypass secret (`GET /v9/projects/{name}` → `.protectionBypass |
  keys[0]`) and send `x-vercel-protection-bypass: <secret>`.
- Vercel ignores Cloudflare-Pages `_redirects`/`_headers`; static dirs need
  `framework=Other` + a `vercel.json` for rewrites/headers. The `agentsfleet.dev`
  root rewrite + `text/x-shellscript` content-type live in
  `ui/agentsfleet.dev/dist/vercel.json` (two explicit header sources — the
  optional-group regex form does not match bare `/`). Deploys ride the git
  integration: preview-on-PR, prod-on-merge.
- Parsing `/v6/deployments` JSON: use `jq` — python's `json.load` chokes on
  control characters in the response.

## Dashboard performance — read dev numbers carefully

The dev-mode `/agents` 1.5–5 s is mostly **not** a backend bug: Turbopack
on-demand route compilation (zero in prod) + local dev calling the **remote**
`api-dev` backend (`lib/api/client.ts` `API_ORIGIN` default) + uncompressed dev
RSC streaming. The `route?_rsc=…` request is the App Router navigation payload,
not the JSON API. The Approvals 5 s repeat is an intentional poll; the Clerk
`/touch`+`/tokens` pair is SDK session management doubled by StrictMode in dev.
The one prod-relevant lever (own perf PR): the server components make 3
*sequential* remote hops (`getToken → workspaces → agents`) — parallelize
billing with workspace resolution and skip the workspaces round-trip when the
`active_workspace_id` cookie is set. Measure a Vercel preview first; never
optimize against dev numbers.

### Authenticated route bundle gate

Build the production app before measuring route size:

```bash
NEXT_PUBLIC_API_URL=http://127.0.0.1:3000 bun run --cwd ui/packages/app build
bun run --cwd ui/packages/app size
```

The report prints framework runtime, the shared authenticated entry, and every
authenticated route. The shared entry must stay at or below 250 Kibibytes
(KiB). Each dashboard route may add at most 100 KiB. Command-line
authentication may total at most 240 KiB.

The size command uses `size-limit` and its file plugin. A dynamic configuration
discovers authenticated routes and emitted chunks from `.next`; only the four
product budgets are maintained. The command fails when a discovered file set
exceeds its limit or a required build manifest is absent or malformed.
Continuous Integration (CI) runs the build and size commands in one job so the
gate measures the output produced immediately before it.

## Synced tooling (not repo-owned)

`scripts/audit-*.sh`, `scripts/lib/`, and `scripts/llmevals/` appear untracked —
they re-sync from the operating-model tooling via `upgrade-ai-tools`. Don't
commit them, don't treat a worktree missing them as data loss, and don't block
worktree cleanup on them.
