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

100 is reachable here rather than aspirational. The Rust crates carry no
input/output, no runtime and no external dependency, so every line is reachable
from a test; the TypeScript packages are pinned at 100 by their own runners. A
floor below what the suite already achieves is slack nobody asked for.

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
