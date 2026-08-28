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

These five are exactly what `.oracle/orly.json` declares and exactly what
`orly gate pr` runs, so the rubric and the mechanical gate grade one boundary.

**Two cadences.** A Section closing is not a milestone closing. Inside a
Section, run `make harness-verify` plus the lane covering the surface the
Section touched (`make test-unit-rustd`, `make test-unit-app`, …) — enough to
prove the Section, never a repository claim. The full table above runs ONCE, at
CHORE(close).

**Which gate runs which row.** `orly gate work` runs the `conform` row alone and
is what the pre-commit hook fires, so it costs seconds. `orly gate verify` runs
the lint, unit, and version rows and is what pre-push fires. `orly gate pr` runs
the integration row plus the whole-branch and spec criteria, and is what
CHORE(close) fires — it skips the fast rows because `git.pushed` proves HEAD is
the commit pre-push already graded.

One lane needs live datastores, and only one. `make/test-infra.mk` brings up
docker compose Postgres and Redis for `test-integration-rustd`; `make
test-unit-all` stays datastore-free, because every Rust test needing one is
`#[ignore]`d and runs only in that lane. `KEEP_TEST_STATE=1` skips the reset for
the inner loop; Continuous Integration (CI) never sets it.

## Test Baseline

`make test-unit-all` reports its own counts per target. Record the cargo
workspace total in the spec header at CHORE(open) as
`**Test Baseline:** unit=<N>`, and compare against it in VERIFY's Test Delta row.
Zero or negative growth on a code-adding diff needs justification or a return to
EXECUTE.

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
