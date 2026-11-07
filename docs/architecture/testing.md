# Test architecture

This page is the canonical source for component ownership and verification topology.

---

## Component ownership

Each component owns its own build graph and test roots. Repository Make targets compose those graphs; they never rebuild a component's import list.

| Component | Lane | Build graph |
|---|---|---|
| `rustd/crates/*/*` (every Rust crate) | `test-unit-rustd` | `rustd/Cargo.toml` |
| `ui/packages/app` | `test-coverage-all` | its own `vitest.config.ts` |
| `ui/packages/website` | `test-coverage-all` | its own `vite.config.ts` |
| `ui/packages/design-system` | `test-coverage-all` | its own `vitest.config.ts` |
| `cli` | `test-coverage-all` | `cli/bunfig.toml` + `cli/scripts/enforce-coverage.mjs` |

Rust integration tests live under each crate's `tests/`. Each file is a binary of its own, or a module that a `*_suite.rs` binary declares. A failing case then shows as the failing test's name, never as a line inside a loop.

Unit tests that must reach a private item stay in `src/` behind `#[cfg(test)]`. Everything reachable through the public surface belongs in `tests/`.

## Public lanes

`make test-unit-all` is the repository's unit claim. It runs `test-unit-rustd`, then `test-coverage-all`, which runs each TypeScript package's own coverage gate.

`test-unit-rustd` is three targets, one per group of layer folders, run in this order:

- `test-unit-rustd-runner`: the `runner/` and `harness/` layers;

- `test-unit-rustd-daemon`: the `agentsfleetd` binary and its suites;

- `test-unit-rustd-daemon-libs`: every other crate, `afd_bench`'s tests included.

Together they cover the whole workspace. Continuous Integration (CI) runs each on its own runner in `.github/workflows/test-unit-rustd.yml`, beside `lint-rustd` and a Miri check of secret wiping. The workflow's last job carries the required check name `test-unit-rustd`, and is green only when every other job was.

A package-scoped runner, such as `cargo test -p af_wire` or `bun run test` inside a package, proves that package and nothing more. It never satisfies the repository claim.

`make test-integration-rustd` needs live datastores, and so does the coverage lane that measures it (§Coverage). Docker compose brings up Postgres and Dragonfly, and the schemas reset per run.

No other lane needs either. `make test-unit-all` stays datastore-free, because every Rust test that needs one is `#[ignore]`d and runs only in those two lanes. `KEEP_TEST_STATE=1` skips the reset for the inner loop; CI never sets it.

`make lint-all` is the lint claim. Its Rust half is `lint-rustd`, which runs the ast-grep rules under `.ast-grep/` through `make lint-source` and the `scripts/rustd_coverage_test.py` self-test. It then runs `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo shear`.

The rest of `lint-all` is the model allowlist check and the TypeScript lints. It also runs the shell, workflow and playbook checks, and the deploy safety gate.

`make test-runner-kernel` proves the runner's sandbox on a real Linux kernel. It builds the toolbox image unless `AFR_TOOLBOX_IMAGE` names one, then runs `afr_sandbox`'s kernel lane as root with bubblewrap, Landlock and cgroup v2. It finishes with `test-runner-startup`, the runner's startup under systemd, and fails rather than skips.

`make check-version` is the version claim: `cli/package.json` and `rustd/Cargo.toml` must match `VERSION`.

`lint-rustd` and `test-unit-rustd` both `cd` into `rustd/` instead of passing `--manifest-path`. Cargo resolves `rust-toolchain.toml` from the working directory. From the repository root, cargo would compile with whatever toolchain the shell has active, and the lane would pass on a compiler nobody agreed to.

The pre-push hook runs `lint-rustd` when a pushed file matches `*.rs` or `rustd/*`. Manifests and the toolchain pin change what the lane compiles and which compiler compiles it, so they trigger it too. The pre-commit hook runs no cargo lane; it routes `*.rs` to `lint-source` only.

## A test calls the code it proves

A test runs the code it names and asserts on what that code does. It never opens a repository file to inspect source text, configuration, workflows, documentation or the committed OpenAPI document.

A test that does goes red when someone reflows a file or rewords a comment. It stays green when the same bug is written another way. A test may read files under its own fixture directory and files it wrote itself.

- **OpenAPI facts** come from the generated document, `afd_api::openapi::document()`, or a type's `<T as utoipa::PartialSchema>::schema()`, never `public/openapi.json`.

- **SQL guarantees** are proven live: run the statement through its store against Postgres and assert on the rows. A substring match on statement text proves nothing about the rows.

- **Rules about how source text reads** (a banned import, a retired word) are lints. Each lives in a tool `make lint-all` runs (oxlint, clippy or an ast-grep rule under `.ast-grep/rules/`), whose own test seeds a violation. A rule no single-file tool can state is not written.

- **Pins** stay only on observable interface: error codes, wire spellings, exit codes, roles and accessible names. Copy, class strings and constants compared with themselves are not interface.

The `tests-*-repo-read` ast-grep rules (`.ast-grep/rules/tests/`, run by `make lint-source`) fail on a test that reads a repository file directly. They cover TypeScript, Rust, shell and Python. A read routed through a variable or a helper function is review's to catch.

There is no allowlist. A false positive is fixed in the rule, with a valid case in its test.

## Rust test naming — the filename declares the tier

Three shapes, and the tier is readable without opening the file.

| Shape | Needs a live datastore | Lane | `#[ignore]` |
|---|---|---|---|
| `integration_<subject>.rs` | yes — Postgres, Dragonfly, or a booted daemon | `make test-integration-rustd` | on every test in the file |
| `<subject>.rs` | no | `make test-unit-rustd` | on nothing in the file |
| `*_suite.rs` | — | — | declares `#[path]` modules only, holds no test of its own |

**The filename and the attribute must agree.** `cargo test` runs the non-ignored tests, and the integration target passes `-- --ignored` to run the rest.

A live test without its `#[ignore]` runs in the unit lane and fails the moment Docker is closed. That is the one failure this rule prevents outright.

**The naming rule is not a safety property.** `#[ignore]` alone decides which lane runs a test; the filename decides nothing. An `#[ignore]`d test in a file with no `integration_` prefix still runs under `-- --ignored`.

So a misfiled test is a readability defect, and it still runs. The naming rule's value is that a directory listing answers "what does this crate prove without a database" before anyone opens a file.

**Reachability is what actually loses a test.** A crate with `autotests = false` compiles only the files a `[[test]]` target names. It also compiles whatever those files pull in with `#[path]`.

A file listed nowhere is no test at all: it compiles in no binary, appears in no count and fails nothing. That is how suites in this repository reached a milestone having never run. It is also how a merge later orphaned eight sslmode tests in `afd_db`.

**Nothing enforces this automatically.** An audit did, briefly, until M185 removed it as more machinery than the hazard was worth. So review checks it.

A new `tests/*.rs` in a crate with `autotests = false` must be declared in that crate's `*_suite.rs`. Without the declaration, the file compiles nowhere and fails nothing.

## Rust test naming — the function is a sentence, not a `test_` label

The filename rule above decides the tier, and this rule decides the function name. M180 settled it by measuring rather than by preference. Two conventions were live at once, and a spec had named seventeen tests in the losing one.

**A test function is named for the behaviour it pins, as a sentence, with no `test_` prefix.** Write `a_signature_under_the_wrong_key_is_refused_on_every_scheme`, not `test_signature_matrix`. The prefix is a carryover from runners that discover tests by name.

`#[test]` performs discovery here, and cargo already prints the module path ahead of the function. So `test_` buys nothing and costs the first five characters of every line of failure output.

**The measurement, so a later reader can re-run it rather than trust it.** Group test functions by the day their file was introduced (`git log --diff-filter=A --follow`). Files from 2026-08-24 through 08-26 are 100% prefixed across 378 tests.

From 08-27 onward the practice inverts, at 14%, 0%, 31% and 9% across 411 tests. The split is the same per crate. Every 100% crate (`afd_auth`, `afd_db`, `afd_identity`, `afd_dragonfly`, `afd_state`) predates the 27th, while `afd_api` sits at 20% and `afd_cron` and `af_webhook` at 0%.

**This rule is no licence to rename.** The prefixed crates are internally consistent, and their names are not wrong, only older. Touching one test in `af_crypto` does not oblige renaming its neighbours, and a crate converts, if ever, in one deliberate commit of its own.

**A new test in an existing file follows that file.** A mixed file is worse than either convention held whole.

So the rule binds new files and new crates. Adding `the_thing_it_proves` beside four `test_`-prefixed neighbours makes the new test the odd one out.

M180 added `test_the_tag_type_offers_no_short_circuiting_comparison` to `af_crypto` for exactly this reason. That name follows the rule's second clause, and is no exception to it.

**Where a spec and this rule disagree, the spec is amended.** A spec names a Dimension's proving test so readers can find it.

A name the codebase does not use fails at exactly that job. M180 amended seventeen Dimensions rather than rename fourteen tests into a convention the repository had already left.

## Test isolation on a shared datastore (rules ISO-1 to ISO-3)

The lane has one Postgres and one Dragonfly, and tests run concurrently inside every test binary. Cargo runs test binaries one at a time, and libtest runs the tests within one in parallel. So splitting files apart changes nothing for two tests in the same file, and file layout satisfies no rule below.

The failure these rules codify had no error in it. Every red was a query that succeeded and returned the truth:

- a count of 2 where a test expected 0, because the rows were a sibling's;

- a gate correctly marked `timed_out`, because a sibling ran the sweeper;

- a chunk delivered to the right subscriber, because a sibling's daemon had leased this test's event.

Shared test data read concurrently gives correct answers to the wrong question, and the assertion is what breaks.

**ISO-1: Mint every identifier a test writes or reads back**. A fixed constant suits only a row nothing asserts over (the fixture tenant, written `ON CONFLICT DO NOTHING`). The moment a test counts, lists or filters by an identifier, that identifier is minted per test.

`Lane::isolated` is the shape: mint the workspace and fleet, seed them, scope every read. It costs a few statements and removes the whole row-collision class.

**ISO-2: Placement reads a deployment-wide set.**

A lease poll can choose any eligible fleet in its database.
A test that asserts absence or exact placement needs a private database or uniquely required runner labels.
A minted fleet identity alone cannot isolate a global candidate scan.

**ISO-3: Exclude global work by design.** `Inbox::expire` runs `UPDATE fleet.approval_gates … WHERE resolved_at IS NULL AND expires_at <= now()` with no workspace or fleet filter, because the sweeper is system-wide.

The actor and tests that depend on its input must both take the same named lock.
Hold the lock for the entire test body. Prefer a fixture-owned guard; `Scenario` holds the placement guard as its last field.

**Which tier applies.** Ask what the assertion reads. One row by its own minted id is ISO-1.

A set (a count, a listing, a queue depth) is ISO-1, and ISO-2 as well if the set lives behind a global key. An assertion whose premise a system-wide writer can invalidate is ISO-3, whatever else is true of it.

**ISO-3 costs wall time.** Prefer private databases or scoped placement labels when a test needs a deployment-wide premise.
Keep the shared guard until both the reader and writer have an isolated replacement.

## The wire

`af_wire` defines the `/v1/runners` protocol. `agentsfleetd` serves it and publishes the shapes through `public/openapi.json`. `openapi_contract.rs`, `openapi_coverage.rs` and their siblings in `afd_api/tests/` grade that document against the routes.

`agentsfleet-runner` is a client of the same crate. It speaks `af_wire`'s types and nothing else, so a shape the two sides disagree on fails to compile ([Runner execution](./runner_execution.md#why-rust)).

## Coverage

The target is **100%**, project-wide and per flag. TypeScript gates at that target today. Rust gates at lower floors that later milestones raise toward it.

| Flag | Paths | Gate |
|---|---|---|
| `rust-afd` | `rustd/crates/` | 98.5% project · 94% patch |
| `typescript` | `app`, `website`, `cli` | 100% |

Every threshold is 0%, so a gate has no give. A patch status grades only the lines a diff touched, so on a small diff one unhit line reds the build. That is intended, and the answer is a test, never absorbed slack.

The Rust gate measures the unit tier and the ignored live-datastore tier together, in three shards: `runner`, `daemon` and `substrate`. `make/test-integration-rustd.mk` lists them once. Continuous Integration runs them in parallel, each on its own runner with its own Postgres and Dragonfly.

Postgres, Dragonfly, HTTP and runtime code are part of the denominator. So each shard resets its lane, applies the schema through the instrumented daemon, and runs its tests with `--include-ignored`. Each shard writes `rustd/lcov-<shard>.info` and grades nothing.

`make test-coverage-rustd-merge` merges the shard reports and the kernel lane's report line by line. It grades the line floor once over the union. A missing report, or one measured at another commit, fails the lane instead of shrinking it.

Locally, `make test-coverage-rustd` without `RUSTD_SHARD` runs every shard in sequence. It grades with the same script, `scripts/rustd_coverage.py`, and Codecov grades the same merged report.

The line floor is `RUSTD_COVERAGE_FLOOR`, 98.5% today, and later milestones raise it toward 100%. The same script grades the patch, the Rust lines a branch adds, against `RUSTD_PATCH_FLOOR`. That floor is 94%, Codecov's `rust-afd` patch target, and moves with it.

Coverage builds differ from normal development builds on purpose. Continuous Integration disables Cargo incremental compilation for this job. No edit-build loop would reuse it, and caching incremental object trees only moves stale gigabytes between runners.

Local development keeps Cargo's incremental default. `[profile.dev] debug = 1` limits debug information to line tables. It neither disables incremental compilation nor clears old fingerprints and build output.

If a later change adds a genuinely untestable line, move the number in the same commit and say why. Do not let it drift down silently.

`rustd/target/`, test files and generated output are all in `codecov.yml`'s `ignore` list, so the published rate describes shipped, measured code only.

## Adding a component

A new component adds its own test roots to its own build graph. It then adds one row to the ownership table above, and one to the coverage table if it publishes a flag. It does not add imports to another component's root.

A new crate joins `rustd/Cargo.toml`'s explicit member list and carries `[lints]` + `workspace = true`, or it silently escapes every deny the workspace declares.
