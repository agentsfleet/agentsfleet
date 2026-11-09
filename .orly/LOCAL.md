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
- Postgres holds every durable fact; the message bus is core NATS and holds
  none, per [the datastore requirements](../docs/architecture/datastore_scaling.md)
  (Indy, Nov 06, 2026, M244_001 Discovery). Three NATS nodes in one region,
  clustered by routes, are the deployment target, with JetStream, KV and the
  object store off everywhere. The crate is `afd_bus`; the boot knob is
  `NATS_URL`, comma-separated seeds with credentials in the userinfo, declared
  at `rustd/crates/daemon/afd_bus/src/nats/config.rs`. Boot refuses, as
  `UZ-STARTUP-004`, a seed it cannot reach and a server that reports JetStream,
  no cluster, no headers or a `max_payload` under the largest frame
  (`afd_bus/src/nats/preflight.rs`). The local lane is a real three-node
  cluster: compose `nats-1`, `nats-2` and `nats-3`.
- Drive work with `orly gate` (work → verify → pr). Hooks run `orly gate work`;
  `orly gate pr` runs by hand at CHORE(close), before `gh pr create`.
  `.orly/orly.json` declares `verify.lint`, `verify.unit`,
  `verify.integration`, and `verify.version` (`make lint-all`,
  `make test-unit-all`, `make test-integration-rustd`, `make check-version`).
  REVIEW remains a separate lifecycle stage.
- **Two lanes need live datastores, and only two.** The Zig integration and
  memory-leak lanes went with the rest of the Zig gating; `make/test-infra.mk`
  survived them, and M176 built `make test-integration-rustd` on it — docker
  compose Postgres and NATS, schemas reset per run. `make test-coverage-rustd`
  is the second: it runs the same `#[ignore]`d tests under the coverage
  instrument with `--include-ignored`. Nothing else a developer runs needs
  either: `make test-unit-all` stays datastore-free, because every Rust test
  that needs one is `#[ignore]`d and runs only in those two lanes.
  `KEEP_TEST_STATE=1` skips the reset for the inner loop; CI never sets it.
- **Make targets are the only repository claims — never hand-roll their
  equivalents.** lint → `make lint-all`
  (Rust lint rides `lint-rustd`) ·
  unit → `make test-unit-all` (cargo workspace + every TypeScript coverage
  gate) · integration → `make test-integration-rustd` (live Postgres + NATS) ·
  version → `make check-version` · dry lanes → `make dry-app` /
  `make dry`. A package-scoped runner
  (`cd ui/packages/app && bun run test`, `cargo test -p af_wire`, …) is
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
  private `ErrorKind`, then `af_core::error_shell!` generates the boxed
  `Error`, its backtrace, its `[CODE]` `Display` and its self-skipping
  `source()`, and `error_lifts!` generates the `From` impls. Only the `Result`
  alias is written by hand, so a reader can see it without expanding a macro.
- **Bounds on untrusted input are declared with `garde`** on the type they
  guard, per HTTP-GARDE and HTTP-VALID in [the HTTP standard](standards/domains/http.md); never a
  hand-written length, range or charset check. Rules garde lacks, the route
  `?limit` (`Limit` + `Ceiling`) and the report-to-sentence table
  (`Sentences`) live in `af_validate`. A parser a bound protects takes
  `&garde::Valid<T>`, so it cannot run on unproved input.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.
- A new script (`scripts/**`, `deploy/**/*.sh`, a `make` recipe's helper) needs
  Indy's sign-off before it is committed; a one-off helper lives outside the
  repository and is deleted when the work ends.
- Test fleets use purpose-driven display names, such as `Support triage` or
  `Pull request review`. Keep run identifiers, timestamps, and cleanup keys out
  of names shown in the sidebar; track fixture ownership by fleet identifier.

## Where the standards' generic words land here

The standards in `.orly/standards/` name things generically. In this
repository:

- **Crates:** each crate sits in one layer folder under `rustd/crates/`:
  `foundation/`, `protocol/`, `harness/`, `runner/` or `daemon/`. It depends
  on its own layer or a lower one, and `runner/` and `daemon/` never on each
  other (`rustd/Cargo.toml`). Shared crates are `af_*`, daemon-only `afd_*`,
  runner-only `afr_*`. A layer's binary sits in `main/` (`daemon/main` is
  `agentsfleetd`, `runner/main` is `agentsfleet_runner`); every other folder
  is its crate's name.
- **SQL:** schema files are `schema/<UTC yyyymmddHHMMSS>_<schema>_<table>.sql`, one
  per table, holding its whole `CREATE` — every column, constraint, index, grant,
  function and trigger. The only `ALTER` is a foreign key that closes a cycle, in
  the later table's file. Until the first production deploy a change to a table
  edits its file; after it, a change is a new `<UTC>_<schema>_<table>_<what>.sql`,
  and `version_of` refuses any other name (Indy, Nov 08, 2026). Statement files
  are `rustd/crates/**/src/**/*.sql`; the migration list is
  `rustd/crates/daemon/afd_db/src/migration.rs`; a foreign-key violation is read with
  `afd_db::constraint::violates_foreign_key`.
- **Identifiers and secrets:** identifiers come from `af_core::id`, secret
  bytes from `af_crypto::entropy`.
  Every column, parameter and array that holds an identifier this system mints
  is a Postgres `uuid`, bound and read as `Uuid7`. `TEXT` holds only external
  identifiers, keys and digests, and no statement casts an identifier between
  the two (Indy, Nov 08, 2026).
- **HTTP:** wire types live in `af_wire`; routes in
  `rustd/crates/daemon/afd_http/src/route/*.rs`; handlers in
  `rustd/crates/daemon/afd_api_*/src/**`; bounds and paging in `af_validate` and
  `af_core::paging`. A new route updates `INVENTORY` in `route_inventory.rs`
  and the counts in `route_meta_total.rs`.
- **Errors:** codes live in `rustd/crates/foundation/af_core/src/error_code/**` with the
  `UZ-` prefix; problem types resolve under
  `https://docs.agentsfleet.net/errors/`. A new code lands as a family const, a
  `REGISTRY` row and a `problem/` entry in one commit. `From` impls come from
  `af_core::error_lifts!`.
- **OpenAPI:** `public/openapi.json` is regenerated from `rustd/` with
  `--no-banner`, never edited by hand.
- **Auth:** `afd_auth`, `afd_credential`, `ui/packages/app/lib/auth/**` and the
  device-session statements in `afd_tenant/src/sql/session/`.
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
- The NATS nodes run `--no_advertise`, so a host client uses its whole seed
  list. Probe a node with `docker compose exec nats-1 wget -q -O -
  http://127.0.0.1:8222/healthz`; recreate the cluster only through
  `make _ensure-test-infra`.
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
