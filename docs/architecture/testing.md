---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Testing: which lane proves which change

> Read this when you add a test, a crate or a package, or need to know which `make` target proves your change. Parent: [`README.md`](./README.md) · Test rules: [`.orly/standards/tests.md`](../../.orly/standards/tests.md).

## What it is

The repository makes four claims, and each is one `make` target.

| Claim | Target | Needs live datastores | Continuous Integration (CI) workflow |
|---|---|---|---|
| lint | `make lint-all` | no | `lint.yml`, with `lint-rustd` in `test-unit-rustd.yml` |
| unit | `make test-unit-all` | no | `test-unit-rustd.yml` for Rust; `test.yml` for each TypeScript package |
| integration | `make test-integration-rustd` | Postgres and NATS | `test-integration-rustd.yml`, which runs the coverage shards |
| version | `make check-version` | no | `lint.yml` |

A package-scoped runner, such as `cargo test -p af_wire` or `bun run test` inside a package, proves that package and nothing more.
It never satisfies a repository claim (TEST-LANE in the tests standard).

## Why it exists

Each component owns its own build graph, so one target can compose them without rebuilding another's imports.
Splitting lanes by what they need keeps the unit claim fast and free of Docker, and puts every live-datastore test in two lanes.

| Component | Lane | Build graph |
|---|---|---|
| every Rust crate under `rustd/` | `test-unit-rustd` | `rustd/Cargo.toml` |
| `ui/packages/app` | `test-coverage-all` | its `vitest.config.ts` |
| `ui/packages/website` | `test-coverage-all` | its `vite.config.ts` |
| `ui/packages/design-system` | `test-coverage-all` | its `vitest.config.ts` |
| `cli` | `test-coverage-all` | `cli/bunfig.toml` and `cli/scripts/enforce-coverage.mjs` |

## How it behaves

### Lanes

`make test-unit-all` runs `test-unit-rustd`, then `test-coverage-all`, which runs each TypeScript package's coverage gate.
`test-unit-rustd` is three shards (`make/test-unit.mk`):

| Shard | Crates |
|---|---|
| `test-unit-rustd-runner` | the `harness/` and `runner/` layers, `guffbolt` and `guffbolt-ai` |
| `test-unit-rustd-daemon` | `agentsfleetd`'s binary crate and the `e2e/` layer |
| `test-unit-rustd-daemon-libs` | the `foundation/` and `protocol/` layers and the daemon libraries |

`make test-integration-rustd` brings up Postgres and three NATS nodes (`ant`, `bee` and `elk`) with Docker compose, and resets the schemas per run (`make/test-infra.mk`).
Every Rust test that needs a datastore is `#[ignore]`d, so only this lane and `make test-coverage-rustd` run it.
`KEEP_TEST_STATE=1` skips the reset for the inner loop; CI never sets it.

`lint-rustd` runs the `scripts/rustd_coverage_test.py` self-test, `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo shear`.
It ends with a standalone `cargo publish --dry-run` of `guffbolt-ai` and `guffbolt` (`make/quality.mk`).
The rest of `lint-all` covers the ast-grep rules, the model allowlist, TypeScript, shell, workflows, playbooks and the deploy safety gate.

`make test-runner-kernel` proves the runner's sandbox on a real Linux kernel, as root, with bubblewrap, Landlock and cgroup v2.
It finishes with `test-runner-startup`, the runner's startup under systemd, and fails rather than skips.
`make check-version` checks that `cli/package.json` and `rustd/Cargo.toml` match `VERSION`.

Rust lanes `cd` into `rustd/` so Cargo reads `rust-toolchain.toml`; from the repository root it would use whatever compiler the shell has.
The pre-push hook runs `lint-rustd` when a pushed path matches `*.rs` or `rustd/*` (`.githooks/pre-push`).

### Test tiers by filename

| File shape | Needs a live datastore | Lane | `#[ignore]` |
|---|---|---|---|
| `integration_<subject>.rs` | yes: Postgres, NATS or a booted daemon | `make test-integration-rustd` | on every test in the file |
| `<subject>.rs` | no | `make test-unit-rustd` | on nothing |
| `*_suite.rs` | none of its own | the lane of what it declares | declares `#[path]` modules only |

`#[ignore]` alone decides the lane; the filename lets a directory listing show which tests need a database.
A live test missing its `#[ignore]` runs in the unit lane and fails as soon as Docker is closed.

In a crate with `autotests = false`, a new `tests/*.rs` must be declared in that crate's `*_suite.rs`.
An undeclared file compiles into no binary and fails nothing, and nothing but review catches it.
Unit tests that need a private item stay in `src/` behind `#[cfg(test)]`; everything reachable through the public surface belongs in `tests/`.

A new test function is named as a sentence with no `test_` prefix (TNM).
A new test in an existing file follows that file's style.

A test calls the code it proves and never reads a repository file; the `tests-*-repo-read` ast-grep rules enforce this (TEST-PUBLIC).
OpenAPI facts come from `afd_api::openapi::document()`, never from `public/openapi.json`.

### Isolation on the shared datastore

The integration lane has one Postgres and one NATS cluster, and tests in one binary run in parallel.
A concurrent sibling's rows give correct answers to the wrong question, so isolation is a rule, not file layout.

| Rule | When it applies | How to meet it |
|---|---|---|
| ISO-1 | the assertion reads rows by identifier, or counts or lists them | mint every identifier the test writes or reads back; `Lane::isolated` mints and scopes a workspace and fleet |
| ISO-2 | the assertion reads a deployment-wide set, such as assignment candidates | a private database, or runner labels no other test uses |
| ISO-3 | a system-wide worker can change the premise, such as the outbound worker claiming `thread.deliveries` lanes | hold the same named lock as the worker for the whole test; `Scenario` holds the placement guard as its last field |

ISO-3 costs wall time, so prefer a private database or scoped labels where a test can use one.

### Coverage

| Language | Paths | Floor | Graded by |
|---|---|---|---|
| Rust | `rustd/crates/`, `rustd/guffbolt/`, `rustd/guffbolt-ai/` | 98 percent, project and patch | `scripts/rustd_coverage.py`, with `RUSTD_COVERAGE_FLOOR` in `make/test-integration-rustd.mk` |
| TypeScript | `app`, `website`, `cli` | 100 percent, project and patch | Codecov, threshold 0 |

The Rust lane measures the unit and live-datastore tiers together, in seven shards: `foundation`, `harness`, `runner`, `daemon-api`, `daemon`, `e2e` and `guffbolt`.
Each shard resets its lane, runs with `--include-ignored` and writes `rustd/lcov-<shard>.info`; CI runs them in parallel.
`make test-coverage-rustd-merge` merges the shards and the kernel lane's report, then grades the floor once over the union.

A missing report, or one measured at another commit, fails the merge.
Codecov reports the `rust-afd` flag without a status, so the floor lives in one place.
`make test-coverage-rustd` without `RUSTD_SHARD` runs every shard in sequence locally.

### The wire

`af_wire` defines the runner protocol, and `agentsfleetd` publishes its HTTP shapes through `public/openapi.json`.
`openapi_contract.rs`, `openapi_coverage.rs` and their siblings in `afd_api/tests/` grade that document against the routes.
`agentsfleet-runner` speaks the same crate's types, so a shape the two sides disagree on fails to compile.

## Limits

- **Nothing checks test reachability.** An undeclared file in an `autotests = false` crate is caught only in review.
- **The floor is not 100 percent for Rust.** A genuinely untestable line moves `RUSTD_COVERAGE_FLOOR` in the same commit, with the reason.
- **Two lanes need Docker.** `make test-integration-rustd` and `make test-coverage-rustd` cannot run without the compose stack.

## Related pages

- [`.orly/standards/tests.md`](../../.orly/standards/tests.md): the test rules
- [`docs/development.md`](../development.md): local setup and the pre-push hook
- [Rust ownership](./rust-ownership.md): which crate owns which mechanism
