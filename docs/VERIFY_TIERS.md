# Verification tiers — `agentsfleet`

The exact commands VERIFY runs, and the block it emits. The generic discipline —
when the gate fires, what a skip must say — lives in `dispatch/verify.md`; this
page carries only what is specific to this repository.

## Tiers

| Tier | Command | When |
|---|---|---|
| conform | `make harness-verify` | Section lane and boundary, after EXECUTE and before the rest. Any 🔴 returns to EXECUTE. |
| lint | `make lint-all` | Boundary. Rust rides `lint-rustd` (rustfmt + Clippy, warnings are errors); script self-tests ride `lint-scripts`. |
| unit | `make test-unit-all` | Boundary. The cargo workspace plus every TypeScript package coverage gate. |
| version | `make check-version` | Boundary. `VERSION` against `build.zig.zon`, `cli/package.json` and both `rustd/Cargo.toml` sites. |
| integration | `make test-integration-rustd` | Boundary. Live Postgres and Redis via docker compose, schemas reset per run. `orly gate pr` skips it on a branch carrying no code. |

These are the commands `.oracle/orly.json` declares. The commit hook runs
conformance; `orly gate pr` runs every verification row.

The authoritative timing and gate ownership live in
`dispatch/lifecycle.md` §What runs at each stage. At Section completion, run
`cargo fmt` from `rustd/`, `make lint-rustd`, conformance over staged changes,
and the relevant behavior tests. The existing `make test-unit-rustd` runs the
whole Rust workspace; it is not a single-package check.

## Test Baseline

Before the Pull Request, obtain `make test-unit-all` and
`make test-integration-rustd` results for the spec's full `Baseline revision`.
Record per-package unit counts and the integration lane's own summary in the
baseline report; put comparable totals in `Test Baseline` and its path or run
URL in `Baseline evidence`. The report includes passed, failed, and skipped
counts, commands, and environment. Never substitute the default checkout's
current revision for the recorded one. Keep integration datastores isolated.

The final PR gate runs all declared commands against the implementation.
Capture its output in Pull Request Session Notes, including the Test Delta.
Unit growth alone does not prove coverage; explain removals and changes in test
selection. Zero or negative growth on code-adding work requires justification.

## Wire fixtures

`make wire-fixtures` regenerates `samples/fixtures/wire-v2/` from
`src/lib/contract` — the Zig module that still defines the `/v1/runners` wire.
Run it whenever a wire type changes, and commit the regenerated fixtures in the
same commit as the type change.

The fixtures are the parity oracle for the Rust port: Zig generates, Rust
conforms, and the suite compares BYTES. Never hand-edit one. A fixture diff with
no type change beside it means someone edited generated output; a type change
with no fixture diff means the emitter never ran.

## Coverage

One bar, everywhere: **100%**, project-wide and per flag.

| Flag | Paths | Target |
|---|---|---|
| `rust-afd` | `rustd/crates/` | 100% |
| `typescript` | `app`, `website`, `cli` | 100% |

Every threshold is 0%: the target IS the bar. The Rust crates carry no
input/output, no runtime and no external dependency, so every line is reachable
from a test; the TypeScript packages are pinned at 100 by their own runners.

The Zig tree is in Codecov's `ignore` list. It still compiles and the revision
built from it serves `api-dev`, but nothing measures it.

## Required output

Paste the deciding line, not the exit code. A gate that reports a number — a size
against a cap, a count against a budget — is owed that number in the done message.

```
✅ Verified: 🧪 lint-all ✓ · 🧪 test-unit-all ✓ <N>p/<M>s · 🧩 test-integration-rustd ✓ · 🔆 harness-verify ✓ · 🔆 check-version ✓
```

A Section lane reports what it ran and says so, never borrowing the repository's
verdict:

```
✅ Section verified: 🔆 harness-verify ✓ · 🧪 test-unit-<surface> ✓ <N>p/<M>s
```

A skipped target is surfaced as 🟠 with its reason, never dressed as a pass.
