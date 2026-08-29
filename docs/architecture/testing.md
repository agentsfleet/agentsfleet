# Test architecture

Canonical component ownership and verification topology.

---

## Component ownership

Each component owns its own build graph and test roots. Repository Make targets
compose those graphs; they never rebuild a component's import list.

| Component | Lane | Build graph |
|---|---|---|
| `rustd/crates/afd_core` | `test-unit-rustd` | `rustd/Cargo.toml` |
| `rustd/crates/afd_wire` | `test-unit-rustd` | `rustd/Cargo.toml` |
| `ui/packages/app` | `test-coverage-all` | its own `vitest.config.ts` |
| `ui/packages/website` | `test-coverage-all` | its own `vite.config.ts` |
| `ui/packages/design-system` | `test-coverage-all` | its own `vitest.config.ts` |
| `cli` | `test-coverage-all` | `cli/bunfig.toml` + `scripts/enforce-coverage.mjs` |

Rust integration tests live under each crate's `tests/`, one binary per file, so
a failing type is the failing test's NAME rather than a line inside a loop. Unit
tests that must reach a private item stay in `src/` behind `#[cfg(test)]`;
everything reachable through the public surface belongs in `tests/`.

### The Zig daemon is frozen and unmeasured

`src/**`, `build.zig` and `build_runner.zig` still compile, and the revision
built from them serves `api-dev`. Nothing grades them: the Zig lint, unit,
coverage, leak and integration lanes were deleted, along with the automatic
deploy, on the deciding fact that there are no production users and the daemon
is being replaced. The tree is in Codecov's `ignore` list, so it cannot move the
published rate in either direction.

Redeploying that frozen revision is a manual `workflow_dispatch` on
`deploy-dev.yml`, and it is the rollback path the Rust cutover depends on.

## Public lanes

`make test-unit-all` is the repository's unit claim. It runs
`test-unit-rustd` (`cargo test --workspace`) and then `test-coverage-all`, which
runs each TypeScript package's own coverage gate. A package-scoped runner —
`cargo test -p afd_wire`, `bun run test` inside a package — proves that package
and nothing more; it never satisfies the repository claim.

`make lint-all` is the lint claim: `lint-rustd` (`cargo fmt --check` plus
`cargo clippy --workspace --all-targets -- -D warnings`), `lint-scripts` (every
`scripts/*_test.py`), the TypeScript lints, the shell and OpenAPI checks, and the
safety gates.

`lint-rustd` and `test-unit-rustd` both `cd` into `rustd/` rather than passing
`--manifest-path`. `rust-toolchain.toml` resolves from the working directory, so
running cargo from the repository root would silently compile with whatever
toolchain the shell has active instead of the pinned one — the lane would pass on
a compiler nobody agreed to.

Both git hooks dispatch on `*.rs` and on `rustd/*`: manifests and the toolchain
pin change what the lane compiles and which compiler compiles it, so they trigger
it too.

## Rust test naming — the filename declares the tier

Three shapes, and the tier is readable without opening the file.

| Shape | Needs a live datastore | Lane | `#[ignore]` |
|---|---|---|---|
| `integration_<subject>.rs` | yes — Postgres, Redis, or a booted daemon | `make test-integration-rustd` | on every test in the file |
| `<subject>.rs` | no | `make test-unit-rustd` | on nothing in the file |
| `<crate>_suite.rs` | — | — | declares `#[path]` modules only, holds no test of its own |

**The filename and the attribute must agree.** `cargo test` runs the
non-ignored tests and the integration target passes `-- --ignored` to run the
rest, so a live test without its `#[ignore]` is a unit lane that fails the
moment Docker is closed — the one failure this rule prevents outright.

**What this rule is NOT is a safety property, and the distinction matters.**
`#[ignore]` alone decides which lane runs a test; the filename decides nothing.
An `#[ignore]`d test in a file with no `integration_` prefix still runs under
`-- --ignored`. So a misfiled test is a readability defect, not a skipped one,
and the value of the naming rule is that a directory listing answers "what does
this crate prove without a database" before anyone opens a file.

**The mechanism that actually loses a test is reachability.** A crate with
`autotests = false` compiles only the files a `[[test]]` target names, plus
whatever those files pull in with `#[path]`. A file listed nowhere is not a
skipped test, it is not a test at all: it compiles in no binary, appears in no
count, and fails nothing. That is how suites in this repository reached a
milestone having never executed. The gate worth writing checks that every
`tests/*.rs` holding a `#[test]` is reachable from a declared target — the
audit currently reports one unreachable file, `afd_runner/tests/support.rs`,
which holds no test and is a support module.

## Test isolation on a shared datastore (rules ISO-1 to ISO-3)

One lane, one Postgres, one Redis, and tests that run concurrently inside every
test binary. Cargo serialises BINARIES and libtest parallelises the tests within
one, so splitting files apart changes nothing about two tests in the same file —
file layout is not an isolation mechanism, and no rule below is satisfied by it.

The failure this codifies had no error in it. Every red was a query that
succeeded and returned the truth: a count of 2 where a test expected 0, because
the rows were a sibling's; a gate correctly marked `timed_out`, because a
sibling ran the sweeper; a chunk delivered to the right subscriber, because a
sibling's daemon had leased this test's event. Shared test data read
concurrently produces correct answers to the wrong question, and the assertion
is what breaks.

**ISO-1 — Mint every identifier a test writes or reads back.** A fixed constant is
admissible only for a row nothing asserts over (the fixture tenant, written
`ON CONFLICT DO NOTHING`). The moment a test counts, lists, or filters by an
identifier, that identifier is minted per test. `Lane::isolated` is the shape:
mint the workspace and fleet, seed them, scope every read. Cost is a few
statements; it removes the whole row-collision class.

**ISO-2 — ISO-1 does not reach a key the product spells globally.** `fleet:ready` is
one hash for the whole deployment and `HRANDFIELD` hands a poller somebody's
fleet at random — competing consumers, which is the design. Minted row ids do
not touch it. Isolation here means a keyspace of the test's own (a Redis logical
database in the connection URL, or a key prefix), and until one exists, ISO-3.

**ISO-3 — Exclude what is global BY DESIGN.** `Inbox::expire` is
`UPDATE … WHERE status = pending AND timeout_at <= $5`: no workspace, no fleet,
correctly, because a sweeper is system-wide. Nothing namespaces that. Those
tests take a named lock — and BOTH sides take it, the actor and the test whose
premise is the actor's input, since a lock one side ignores separates nothing.
Hold it for the test body; prefer a guard the fixture owns (`Scenario` holds the
ready-stream guard as its last field) over a line every test must remember.

**Which tier applies.** Ask what the assertion reads. One row by its own minted
id is ISO-1. A SET — a count, a listing, a queue depth — is ISO-1, and ISO-2 as well if
the set lives behind a global key. An assertion whose premise a system-wide
writer can invalidate is ISO-3, whatever else is true of it.

**ISO-3 is a debt, not a resting place.** Serialisation buys correctness with wall
time: the daemon end-to-end scenarios are exclusive over `fleet:ready`, and the
lane measured `tests_s` 110-123 with the guard against 71 for the concurrent
runs that were producing wrong answers. Recovering that time means ISO-2 for the
ready stream, not removing the guard.

## The wire parity proof

`afd_wire` is a port of a wire the Zig `src/lib/contract` module still defines,
so it is verified against that module rather than against itself.

`src/lib/contract/fixture_export.zig` writes one canonical JSON document per
exported wire type into `samples/fixtures/wire-v2/`, plus a machine-readable
`manifest.json`. `make wire-fixtures` regenerates them. The Rust suite parses each
fixture, re-serializes it, and compares **bytes**.

Three properties make that comparison mean something:

- **Zig generates, Rust conforms.** If Rust produced the fixtures, a Rust bug
  would be baked into the expected bytes and the suite would pass forever. The
  generator has to be the other implementation or the oracle is circular.
- **Bytes, not fields.** Field equality would miss field ORDER,
  optional-emission policy, number spelling and enum spelling — every way two
  encoders agree on a value and disagree on its encoding.
- **The roster is reflection, not a list.** The emitter walks what the contract
  modules actually export. A hand-written list is one someone forgets to update,
  and a forgotten wire type is the drift the fixtures exist to catch.

Two things stay hand-maintained, being what reflection cannot know: the excluded
modules, and the per-type unknown-field policy. That policy is genuinely mixed —
the Zig daemon passes `ignore_unknown_fields` at some parse sites and not others —
so the manifest records it per type and the Rust serde attributes mirror it,
with a generated probe per type asserting the observed leniency matches.

What the round-trip **cannot** prove is integer width in the widening direction:
any value Zig emits fits a wider Rust type and re-serializes identically. That
gap is named in the tests and closed by a separate assertion that a value one past
each declared width is refused.

Fixtures are generated output. Never hand-edit one; regenerate and review the
diff.

## Coverage

One target, one bar: **100%**, project-wide and per flag.

| Flag | Paths | Target |
|---|---|---|
| `rust-afd` | `rustd/crates/` | 100% |
| `typescript` | `app`, `website`, `cli` | 100% |

Every threshold is 0%: the target IS the bar, with no give. A patch status grades
only the lines a diff touched, so on a small diff one unhit line reds the build —
intentionally. The answer is a test, never absorbed slack.

The Rust target measures the unit tier and the ignored live-datastore tier in
one `cargo llvm-cov` invocation. Postgres, Redis, HTTP and runtime code are part
of the denominator, so `make test-coverage-rustd` resets the lane, applies the
schema through the instrumented daemon, then runs both tiers once with
`--include-ignored`. The target writes `rustd/lcov.info` and enforces the same
100% line floor locally before Codecov upload. A floor below the published
contract would leave local verification and the remote status grading different
claims.

Coverage builds are intentionally distinct from normal development builds.
Continuous Integration disables Cargo incremental compilation for this job:
there is no edit-build loop to reuse, and caching incremental object trees only
moves stale gigabytes between runners. Local development retains Cargo's
incremental default. `[profile.dev] debug = 1` limits debug information to line
tables; it does not disable incremental compilation or garbage-collect old
fingerprints and artifacts.

If a later change adds a genuinely untestable line, move the number in the same
commit and say why. Do not let it drift down silently.

The Zig tree, `rustd/target/`, test files and generated output are all in
`ignore`, so the published rate describes shipped, measured code only.

## Adding a component

A new component adds its own test roots to its own build graph, then adds one row
to the ownership table above and one to the coverage table if it publishes a flag.
It does not add imports to another component's root.

A new crate joins `rustd/Cargo.toml`'s explicit member list and carries
`[lints]` + `workspace = true`, or it silently escapes every deny the workspace
declares — `test_workspace_lint_policy` fails the build if it does not.
