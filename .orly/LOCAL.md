# `agentsfleet` repository instructions

The operating model is committed here. `orly init` materialised it —
`.orly/AGENTS.md`, the `.orly/standards/` rule files, and the `orly-spec`,
`orly-build` and `orly-ship` skills — and `.orly/orly.json` records the engine version and every file it wrote.
Nothing resolves out of a developer's home directory, so a fresh clone reads
its own rules. `orly update --no-hooks` re-
materialises them; `orly doctor` reports drift. This file carries only project
facts.

- Write the product as `agentsfleet`; binaries are `agentsfleetd` and
  `agentsfleet-runner`. API entities use `fleet`, `fleet_id`, and `/fleets`.
- The datastore is Dragonfly, cluster-only, per [the datastore requirements](../docs/architecture/datastore_scaling.md).
  `agentsfleetd` speaks one transport — redis-rs `cluster_async` over RESP3 — with no standalone
  path and no topology selector; a seed that is not a cluster refuses boot. Four self-hosted
  Dragonfly processes in one region are the deployment target, with Dragonfly Cloud Swarm the
  later move once its control plane is worth its bill (Indy, 2026-09-14, superseding the earlier
  "Swarm is the required target" rule); the local lane is a real four-node cluster. Redis is not a supported
  backend: Indy called the cutover on 2026-09-12 (M192_001 Discovery), superseding the earlier
  "Redis remains the default" rule. The crate is `afd_dragonfly`; the boot knob is
  `DRAGONFLY_URL`, declared at `rustd/crates/afd_dragonfly/src/config.rs`. The retired Zig
  daemon's `REDIS_*` spelling went with it — one URL for both roles, and a seed that answers
  as a single server is refused at `serve/runtime.rs` before the daemon serves anything.
- Drive work with `orly gate` (work → verify → pr). Hooks run `orly gate work`;
  `orly gate pr` runs by hand at CHORE(close), before `gh pr create`.
  `.orly/orly.json` declares `verify.lint`, `verify.unit`,
  `verify.integration`, and `verify.version` (`make lint-all`,
  `make test-unit-all`, `make test-integration-rustd`, `make check-version`).
  REVIEW remains a separate lifecycle stage.
- **One lane needs live datastores, and only one.** The Zig integration and
  memory-leak lanes went with the rest of the Zig gating; `make/test-infra.mk`
  survived them, and M176 built `make test-integration-rustd` on it — docker
  compose Postgres and Dragonfly, schemas reset per run. Nothing else a developer
  runs needs either: `make test-unit-all` stays datastore-free, because every
  Rust test that needs one is `#[ignore]`d and runs only in that lane.
  `KEEP_TEST_STATE=1` skips the reset for the inner loop; CI never sets it.
- **Make targets are the only repository claims — never hand-roll their
  equivalents.** lint → `make lint-all`
  (Rust lint rides `lint-rustd`) ·
  unit → `make test-unit-all` (cargo workspace + every TypeScript coverage
  gate) · integration → `make test-integration-rustd` (live Postgres + Dragonfly) ·
  version → `make check-version` · dry lanes → `make dry-app` /
  `make dry`. A package-scoped runner
  (`cd ui/packages/app && bun run test`, `cargo test -p afd_wire`, …) is
  inner-loop iteration; it proves a package, not the repository, and never
  satisfies a VERIFY row or a "tests pass" claim.
- A fresh linked worktree requires `bun install`, followed by
  `(cd cli && bun install && bun run build)` **and a `bun install` inside each
  `ui/packages/*` package** before repository tests. The root install alone
  leaves `ui/packages/app` short of its own dependencies, and
  `make test-coverage-all` then fails resolving `next/headers` — a failure that
  looks like a code defect and is not one.
  `provision-env-1password` (dotfiles) writes machine-level environment files
  under `~/.config/agentsfleet/`; its shared `.env` exports their paths for
  local tests. The app throws on an unset `NEXT_PUBLIC_API_URL` instead of
  guessing a backend.
- **Rust errors follow [`docs/RUST_ERROR_STANDARD.md`](../docs/RUST_ERROR_STANDARD.md)** —
  read it before adding or changing a fallible signature under `rustd/`. The
  four rules and their examples are ERR-RS, RS-FROM, RS-MAPERR and RS-SOURCE in
  `.orly/standards/languages/rust.md`; the standard is what this repository
  does differently.
  Carry one fact in: **a crate never hand-writes its error type.** Declare a
  private `ErrorKind`, then `afd_core::error_shell!` generates the boxed
  `Error`, its backtrace, its `[CODE]` `Display` and its self-skipping
  `source()`, and `error_lifts!` generates the `From` impls. Only the `Result`
  alias is written by hand, so a reader can see it without expanding a macro.
- **Bounds on untrusted input are declared with `garde`** on the type they
  guard, per HTTP-GARDE and HTTP-VALID in [the HTTP standard](standards/domains/http.md); never a
  hand-written length, range or charset check. Rules garde lacks, the route
  `?limit` (`Limit` + `Ceiling`) and the report-to-sentence table
  (`Sentences`) live in `afd_validate`. A parser a bound protects takes
  `&garde::Valid<T>`, so it cannot run on unproved input.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.
- Test fleets use purpose-driven display names, such as `Support triage` or
  `Pull request review`. Keep run identifiers, timestamps, and cleanup keys out
  of names shown in the sidebar; track fixture ownership by fleet identifier.

## Where the standards' generic words land here

The standards in `.orly/standards/` name things generically. In this
repository:

- **SQL:** schema slots are `schema/*.sql`; statement files are
  `rustd/crates/**/src/**/*.sql`; the migration list is
  `rustd/crates/afd_db/src/migration.rs`; a foreign-key violation is read with
  `afd_db::constraint::violates_foreign_key`.
- **Identifiers and secrets:** identifiers come from `afd_core::id`, secret
  bytes from `afd_crypto::entropy`.
- **HTTP:** wire types live in `afd_wire`; routes in
  `rustd/crates/afd_http/src/route/*.rs`; handlers in
  `rustd/crates/afd_api_*/src/**`; bounds and paging in `afd_validate` and
  `afd_core::paging`. A new route updates `INVENTORY` in `route_inventory.rs`
  and the counts in `route_meta_total.rs`.
- **Errors:** codes live in `rustd/crates/afd_core/src/error_code/**` with the
  `UZ-` prefix; problem types resolve under
  `https://docs.agentsfleet.net/errors/`. A new code lands as a family const, a
  `REGISTRY` row and a `problem/` entry in one commit. `From` impls come from
  `afd_core::error_lifts!`.
- **OpenAPI:** `public/openapi.json` is regenerated from `rustd/` with
  `--no-banner`, never edited by hand.
- **Auth:** `afd_auth`, `afd_credential`, `ui/packages/app/lib/auth/**` and the
  Dragonfly `session/verify_consume.lua` script.
- **Events:** test helpers come from the `afd_events` `test-util` feature.

## Running lanes here

- Run one Rust lane per machine. On the 32 GB cloud machine, disable development
  and test debug info and incremental compilation; limit Cargo to two jobs.
  Use `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
  `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2`. Check free space before each
  lane. Reclaim build space with `cargo clean`, never `trash`.
- Cargo results count only from `rustd/` with `--all-features` over a lane's
  crate set. Retry a `-p` failure workspace-wide; `--no-fail-fast` goes before
  `--`.
- A red after `KEEP_TEST_STATE=1` or an ad-hoc run counts only after
  `make _reset-test-db`, `make _migrate-test-db` and a clean rerun.
- An ad-hoc `#[ignore]` test needs make's exported environment: an "X is
  unset" panic is an invocation error. `AFD_TEST_LOG=1` shows daemon logs.
- A green PR proves neither Playwright (`make dry-app`) nor acceptance (it runs
  after merge), so a rendered-page change runs its specs locally.
- Run one lane per checkout. Stop a run by its process ID, never `pkill -f`. A
  vitest coverage-lock red is not a test failure.
- Dragonfly nodes announce 127.0.0.1, so probe inside the container with
  `redis-cli -c`. Recreate the cluster only through `make _ensure-test-infra`,
  and force-recreate after changing `scripts/dragonfly-cluster.sh`.
- Linux-only `afr_sandbox` and `afr_executor` code is linted and tested on
  Linux; a Codex cloud task provides one.
- cargo-mutants runs `--in-place` in a throwaway worktree.
- Pre-commit grades the staged index, so commit whole files.
- Never commit the `next-env.d.ts` that `next dev` rewrites. Local `next dev`
  calls the dev API, which runs `origin/main`.
- `ui/packages/app/app/icon.svg` is a copy of `brand.svg`, never a symlink.
- Docs-repository API pages for a new route are committed after the
  `agentsfleet` PR merges.
- An orly pin mismatch is fixed by installing into a prefix the task owns,
  never by changing the global install.
