# `agentsfleet` repository instructions

The operating model is committed here. `orly init` materialised it —
`AGENTS.orly.md`, the `dispatch/` rule pages, and the `audits/` gate scripts —
and `.oracle/orly.json` records the engine version and every file it wrote.
Nothing resolves out of a developer's home directory, so a fresh clone reads
its own rules and runs its own gates. `bunx @agentsfleet/orly update` re-
materialises them; `orly doctor` reports drift. This file carries only project
facts.

- Write the product as `agentsfleet`; binaries are `agentsfleetd` and
  `agentsfleet-runner`. API entities use `fleet`, `fleet_id`, and `/fleets`.
- Drive work with `orly gate` (work → verify → pr). Hooks run `orly gate work`;
  `orly gate pr` runs by hand at CHORE(close), before `gh pr create`.
  `.oracle/orly.json` declares `conform`, `verify.lint`, `verify.unit`,
  `verify.integration`, and `verify.version`. `make harness-verify` satisfies
  CONFORM only; behavioral verification uses the profile's `verify.*` commands
  (`make lint-all`, `make test-unit-all`, `make test-integration-rustd`,
  `make check-version`). REVIEW remains a separate lifecycle stage.
- **One lane needs live datastores, and only one.** The Zig integration and
  memory-leak lanes went with the rest of the Zig gating; `make/test-infra.mk`
  survived them, and M176 built `make test-integration-rustd` on it — docker
  compose Postgres and Redis, schemas reset per run. Nothing else a developer
  runs needs either: `make test-unit-all` stays datastore-free, because every
  Rust test that needs one is `#[ignore]`d and runs only in that lane.
  `KEEP_TEST_STATE=1` skips the reset for the inner loop; CI never sets it.
- **Make targets are the only repository claims — never hand-roll their
  equivalents.** CONFORM → `make harness-verify` · lint → `make lint-all`
  (Rust lint rides `lint-rustd`, script self-tests ride `lint-scripts`) ·
  unit → `make test-unit-all` (cargo workspace + every TypeScript coverage
  gate) · integration → `make test-integration-rustd` (live Postgres + Redis) ·
  version → `make check-version` · dry lanes → `make dry-app` /
  `make dry` · wire fixtures → `make wire-fixtures`. A package-scoped runner
  (`cd ui/packages/app && bun run test`, `cargo test -p afd_wire`, …) is
  inner-loop iteration; it proves a package, not the repository, and never
  satisfies a VERIFY row or a "tests pass" claim.
- A fresh linked worktree requires `bun install`, followed by
  `(cd cli && bun install && bun run build)` **and a `bun install` inside each
  `ui/packages/*` package** before repository tests. The root install alone
  leaves `ui/packages/app` short of its own dependencies, and
  `make test-coverage-all` then fails resolving `next/headers` — a failure that
  looks like a code defect and is not one.
  `.githooks/post-checkout` links `ui/packages/app/.env.local` and
  `.env.runner.local` from `~/.config/agentsfleet/`; a ⚠ from the hook
  means run `provision-env-1password` (dotfiles) first. The app throws on
  an unset `NEXT_PUBLIC_API_URL` instead of guessing a backend.
- **Rust errors follow [`docs/RUST_ERROR_STANDARD.md`](docs/RUST_ERROR_STANDARD.md)** —
  read it before adding or changing a fallible signature under `rustd/`. One
  error type per crate with a `pub type Result<T, E = Error>` beside it;
  compose with `#[from]` so `?` lifts; `map_err` only to ADD context the call
  site alone knows, never `.map_err(|e| Mine(e.to_string()))` — that destroys
  the `source()` chain; and `source()` returns what caused you, never your own
  kind. Not every error has a cause, so a test demanding one for every variant
  is wrong. The shape is `core_api`'s, bun's and habitat's, not invented here.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.

<!-- orly:begin -->
**Engineering harness:** read [`AGENTS.orly.md`](AGENTS.orly.md) as well — it carries the safety rules,
the dispatch router that names which rule page to read before which edit, and the lifecycle
this repository gates on. Where the two disagree, this file wins.
<!-- orly:end -->
