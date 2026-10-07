# `agentsfleet` repository instructions

The operating model is committed here. `orly init` materialised it —
`.orly/AGENTS.md`, the `.orly/dispatch/` rule pages, and the `.orly/audits/` gate scripts —
and `.orly/orly.json` records the engine version and every file it wrote.
Nothing resolves out of a developer's home directory, so a fresh clone reads
its own rules and runs its own gates. `orly update --no-hooks` re-
materialises them; `orly doctor` reports drift. This file carries only project
facts.

- Write the product as `agentsfleet`; binaries are `agentsfleetd` and
  `agentsfleet-runner`. API entities use `fleet`, `fleet_id`, and `/fleets`.
- The datastore is Dragonfly, cluster-only, per [the datastore requirements](docs/architecture/datastore_scaling.md).
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
  `.orly/orly.json` declares `conform`, `verify.lint`, `verify.unit`,
  `verify.integration`, and `verify.version`. `make harness-verify` satisfies
  CONFORM only; behavioral verification uses the profile's `verify.*` commands
  (`make lint-all`, `make test-unit-all`, `make test-integration-rustd`,
  `make check-version`). REVIEW remains a separate lifecycle stage.
- **One lane needs live datastores, and only one.** The Zig integration and
  memory-leak lanes went with the rest of the Zig gating; `make/test-infra.mk`
  survived them, and M176 built `make test-integration-rustd` on it — docker
  compose Postgres and Dragonfly, schemas reset per run. Nothing else a developer
  runs needs either: `make test-unit-all` stays datastore-free, because every
  Rust test that needs one is `#[ignore]`d and runs only in that lane.
  `KEEP_TEST_STATE=1` skips the reset for the inner loop; CI never sets it.
- **Make targets are the only repository claims — never hand-roll their
  equivalents.** CONFORM → `make harness-verify` · lint → `make lint-all`
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
- **Rust errors follow [`docs/RUST_ERROR_STANDARD.md`](docs/RUST_ERROR_STANDARD.md)** —
  read it before adding or changing a fallible signature under `rustd/`. The
  four rules and their examples are in `.orly/dispatch/write_rust.md`, which fires on
  every `*.rs` edit; the standard is what this repository does differently.
  Carry one fact in: **a crate never hand-writes its error type.** Declare a
  private `ErrorKind`, then `afd_core::error_shell!` generates the boxed
  `Error`, its backtrace, its `[CODE]` `Display` and its self-skipping
  `source()`, and `error_lifts!` generates the `From` impls. Only the `Result`
  alias is written by hand, so a reader can see it without expanding a macro.
- **Bounds on untrusted input are declared with `garde`** on the type they
  guard, per [the REST guide §8](.orly/docs/REST_API_DESIGN_GUIDELINES.md); never a
  hand-written length, range or charset check. Rules garde lacks, the route
  `?limit` (`Limit` + `Ceiling`) and the report-to-sentence table
  (`Sentences`) live in `afd_validate`. A parser a bound protects takes
  `&garde::Valid<T>`, so it cannot run on unproved input.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.

<!-- orly:begin -->
**Engineering harness:** read [`.orly/AGENTS.md`](.orly/AGENTS.md) as well — it carries the safety rules,
the dispatch router that names which rule page to read before which edit, and the lifecycle
this repository gates on. Where the two disagree, this file wins.

The line below is an import, not decoration: a runtime that resolves it loads those rules
with this file, and one that does not still has the link above.

@.orly/AGENTS.md
<!-- orly:end -->
