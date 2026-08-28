# M184_001: A crate you cannot split by moving files is a crate with a cycle in it

**Prototype:** v2.0.0
**Milestone:** M184
**Workstream:** 001
**Date:** Aug 27, 2026
**Status:** DONE
**Priority:** P1 — no behaviour changes, and every build in the repository pays for this one until it lands
**Categories:** API
**Batch:** B8 — NOT parallel with anything touching `afd_fleet`; see Dependencies
**Branch:** `feat/m178-tenant-workspace-surface` — folded into M178's branch at Indy's direction rather than taken as its own tree; see Discovery
**Test Baseline:** `unit=1221 integration=144` — `cargo test --workspace --all-features` at `2839efc18`, and the M176 rustd integration lane's last green count. This milestone's own oracle is that neither number falls.
**Depends on:** M178_001 — hard, and for two reasons. Its §3–§6 add handlers over `vault`, `secrets`, `gate` and `sweep`, which are the modules this spec restructures; and it CREATES `afd_tenant`, so a branch taken from `main` before it merges would be decomposing a crate graph that no longer exists.
**Provenance:** LLM-drafted (Claude Opus 5, Aug 27, 2026)
**Canonical architecture:** `docs/architecture/direction.md` §Two daemons, one contract

---

## Overview

**Result: `afd_fleet` 25,520 → 8,044 lines, across six crates, with no
dependency inversion and no trait introduced.** The measured plan below was
right that the crate had to split and wrong about what was holding it together;
what that turned out to be is recorded in Discovery.

`afd_fleet` is 25,546 lines across 130 files — after M178 already carved
`afd_tenant` out of it — which is roughly four times the next largest crate in
the workspace. A crate is the unit of both parallel compilation and incremental
rebuild, so an edit to any 400-line module inside it rebuilds all 25,000 lines,
and nothing downstream can start until that finishes.

M178 removed the part that could be removed by moving files. What is left cannot
be, and this spec is about why: the remaining modules form a dependency graph
with cycles in it, and Rust crates cannot be circular. Splitting them is
dependency inversion, not a file move.

## PR Intent & comprehension handshake

**One sentence:** break the cycles inside `afd_fleet` so it can become three
crates, changing no behaviour and no wire field.

**The measured graph, as of this authoring:**

| module | lines | depends on |
|---|---|---|
| `lease` | 4482 | credential, gate, memory, money, policy, provider, runner, secrets, vault |
| `gate` | 3279 | lease, policy |
| `credential` | 3048 | gate, lease, provider, secrets, vault |
| `provider` | 2070 | lease, money, vault |
| `sweep` | 1786 | — |
| `runner` | 1718 | — |
| `policy` | 1663 | money, provider, secrets |
| `money` | 1636 | lease, runner |
| `secrets` | 795 | provider, vault |
| `memory` | 561 | lease |
| `vault` | 405 | credential |
| `bundle` | 364 | — |

**The cycles, which are the whole problem:**

```
  lease ──► credential ──► lease
  lease ──► gate ──► lease
  lease ──► money ──► lease
  lease ──► policy ──► money ──► lease
  provider ──► vault ──► credential ──► provider
  secrets ──► provider ──► vault ──► credential ──► secrets
```

Every one of the four biggest modules sits on a cycle. `sweep`, `runner` and
`bundle` — 3,868 lines between them — depend on nothing and are the only part
that moves without thought.

**Comprehension handshake.** An agent picking this up should be able to state,
before writing anything: which direction each cycle gets inverted, and what
carries the inverted call — a trait, a value, or a moved type. An agent that
starts by creating directories has not understood the task.

## Implementing agent — read these first

- `docs/RUST_ERROR_STANDARD.md` — each new crate gets ONE error type; the
  composition rules are what make three of them cheaper than one shared enum.
- `rustd/crates/afd_tenant/` — the worked example. M178 did this split for the
  acyclic quarter; its `error/mod.rs` and the `Refusable` trait in
  `afd_api::handler` show the shape a second and third crate should match.
- `rustd/Cargo.toml` — the M-CRATES-FLAT-FOLDER layout every new member joins.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_fleet/**` | EDIT | the crate being decomposed; what remains is the shared domain core |
| `rustd/crates/afd_gate/**` | CREATE | **built as `afd_gate`, not `afd_lease`** — policy + gate, what a fleet may do and where a run stops for a human. `lease` stayed in the rump, so naming the crate after it would have named the wrong half |
| `rustd/crates/afd_credential/**` | CREATE | **built as `afd_credential`, not `afd_vault`** — vault, secrets, provider, credential broker. The planned name is taken by M178 §4's operator secret surface |
| `rustd/crates/afd_billing/**` | CREATE | money + the billing SQL, plus `afd_tenant`'s wallet and charge-ledger reader. A billing binary must not have to link the api-key and login plane to serve a balance |
| `rustd/crates/afd_runner/**` | CREATE | runner + sweep + `fleet.runners`/`fleet.runner_events` — the HOST's plane where `afd_fleet` is the RUN's |
| `rustd/crates/afd_events/**` | CREATE | `core.fleet_events`, which had no owner: `afd_approval` carried a byte-identical copy of its insert rather than depend on the runner plane |
| `rustd/crates/afd_core/**` | EDIT | the `error_shell!` macro every new crate is born on, the shared datastore sentence, and two vocabularies that moved below their planes |
| `rustd/crates/afd_tenant/**` | EDIT | its billing reader left for `afd_billing` |
| `rustd/crates/afd_api/**` | EDIT | `Refusable` gains the new planes' error types; imports repoint |
| `rustd/crates/agentsfleetd/**` | EDIT | `ServingPlane` assembles stores from three crates rather than one |
| `rustd/Cargo.toml` + `rustd/Cargo.lock` | EDIT | new members |
| `docs/v2/pending/M184_001_P1_API_FLEET_CRATE_DECOMPOSITION.md` | EDIT | this spec: status, baseline, Discovery log |

## Applicable Rules

- RULE SQLMOD — each new crate carries its own `sql/` module tree; a statement
  moves with the code that runs it.
- RULE UFS — a constant duplicated across the split is the failure mode to watch
  for; the gate catches it, and the fix is a shared crate, never a second copy.
- RULE ECL — an error crossing a new crate boundary composes with `#[from]`; a
  `map_err(|e| Mine(e.to_string()))` at a seam destroys the `source()` chain.

## Applicable Gates

`orly gate work` at every commit; `orly gate pr` at close. `make harness-verify`,
`make lint-all`, `make test-unit-all`, `make test-integration-rustd`,
`make check-version` — the declared set, unchanged. This milestone's own oracle
is that the test COUNT does not move: a decomposition that changes a number has
changed behaviour.

## Prior-Art / Reference Implementations

- `rustd/crates/afd_tenant` — M178's extraction of the acyclic quarter, its
  error type, and the `Refusable` trait that let one HTTP edge serve two planes.
- `~/Projects/oss/bun` — the workspace mechanics this repository's virtual
  manifest already follows.

## Sections (implementation slices)

### §1 — The free three — DONE

`sweep`, `runner` and `bundle` depend on nothing. They move first, alone, so the
mechanical half of the work is landed and reviewable before any cycle is
touched.

- **Dimension 1.1** — DONE, and larger than planned. `runner` + `sweep` became
  `afd_runner` (4,327) with `fleet.runners` and `fleet.runner_events`; `money`
  went with them as `afd_billing`, which the spec's graph had wrongly placed on
  a cycle (`money → lease` was a doc link). `bundle` stayed in the `afd_fleet`
  rump — it depends on nothing and nothing outside the lease plane reads it, so
  a crate of 364 lines would have been a compilation unit for its own sake.

### §2 — Invert `lease ↔ credential` — DONE, by moving one file

`lease` calls the broker to mint a credential for a claim; `credential` reads
the lease to scope what it mints. The inversion candidate is the second
direction: what `credential` needs is not the lease STORE but the lease's
identity and reach, which is a value.

- **Dimension 2.1** — DONE, and the inversion was already written. `MintScope`
  is documented as "everything the mint reads out of the lease it was handed" —
  a pure value of `Uuid7`, `Box<str>` and a binding. What actually coupled the
  two was `credential/mint.rs`: 266 lines of `impl Plane`, an inherent impl on
  lease's own type that the ORPHAN RULE already required to live in lease's
  crate. It was misfiled, not entangled. Moving it to `lease/mint.rs` removed
  `credential → lease` and `credential → gate` in one step.
- **Dimension 2.2** — DONE. Tarjan over `use crate::` statements is the
  assertion, run at each step; the final graph is a six-level DAG with every
  strongly-connected component a single module.

### §3 — Invert `lease ↔ gate ↔ policy ↔ money` — DONE, no inversion needed

The claim path's own cluster. These four are one plane and are candidates to
stay together in `afd_lease` rather than be separated — in which case the only
cycle to break is the one crossing OUT of the group.

- **Dimension 3.1** — DONE, and the group is three not four. `money` left first
  as `afd_billing`; `policy` + `gate` became `afd_gate` (5,388) and `lease`
  stayed in the rump. The cycle the spec names did not exist in code: `gate →
  lease` was a doc link (`` [`Admission`](crate::lease::Admission) `` in prose),
  which a grep for `crate::` counts and the compiler never sees. The one real
  edge was `sql → gate`, and it dissolved when `sql/gate.rs` and `sql/grant.rs`
  moved to the plane that runs them (RULE SQLMOD).

### §4 — Invert `provider ↔ vault ↔ secrets ↔ credential` — DONE, one constant

The sealed-material cluster, same question as §3: do these four separate, or are
they one crate with clean edges out?

- **Dimension 4.1** — DONE, as **`afd_credential`** (6,351). The spec's name is
  unavailable: M178 §4 created `afd_vault` for the OPERATOR's secret surface —
  the sealed write, the list that never decrypts, the reference lock over a
  delete — and this is the RUNNER's reader, which opens a declared credential
  and never lists. Two failure policies over one table, deliberately apart.
  The whole cycle was `vault → credential`: one import of
  `FIELD_REFRESH_TOKEN`, an RFC 6749 field name that a vault handle is written
  with by one plane and read with by the other. It moved to
  `afd_core::credential`, below both, exactly as the event column spellings
  did.

### §5 — One error type per crate — DONE

Each new crate gets its own, composed at the seams with `#[from]`, and
`afd_api::handler::Refusable` gains an impl per plane.

- **Dimension 5.1** — DONE, and stronger than "does not match". Each new crate
  owns one error type, and the plane above composes it in with `#[from]` and
  DELEGATES classification: `afd_fleet::Error::code()` calls `source.code()`
  rather than restating a mapping that could drift, and `afd_gate` does the
  same for the credential and billing errors beneath it. `afd_api`'s
  `Refusable` trait took three new planes without changing shape, which is what
  that trait was for. A shared `afd_core::error_shell!` macro landed first, so
  the new crates were born on it rather than hand-writing a seventh copy of the
  hull — that alone deleted 381 lines across the six crates that predate them.

## Interfaces

No HTTP surface changes. No wire type changes. No schema changes. The only
interface that moves is between Rust crates, and it moves from `pub` items in
one crate to `pub` items in three.

## Failure Modes

- **A cycle broken by cloning a type rather than inverting a call.** Two copies
  of one struct that must agree, with nothing enforcing it. The review question
  for every inversion is "what single value now carries what used to be a call".
- **An error seam that flattens its cause.** `map_err` to a string at a crate
  boundary reads as tidy and destroys the `source()` chain an operator needs.
- **A test count that moves.** In either direction. Down means coverage was lost
  in the move; up means behaviour was added under cover of a refactor.
- **A crate boundary that buys nothing.** Splitting a chain into more of a chain
  leaves the serial build cost where it was — see Metrics review, where this
  milestone bought 7% of clean-build time against an expected 34%. The
  incremental loop improved regardless, which is the win that survived.

## Invariants

1. **Behaviour is unchanged.** Every route answers the same status, code and
   sentence before and after. **Held with one declared exception**: six crates
   were answering an invented sentence for an unreachable datastore where every
   ported crate answers the Zig's. Converging them CHANGED what three planes
   say, and the change was from wrong to right — see Declared divergences.
2. **The test count is unchanged.** This is the milestone's own oracle.
3. **No cycle survives.** Enforced by the compiler; a cycle simply will not
   build.

## Metrics & Observability

No new events. The metric this milestone moves is build time, which is not
instrumented and does not need to be: `cargo build --timings` before and after,
recorded in Discovery, is the evidence.

## Test Specification (tiered)

- **Tier 1 (unit):** unchanged. Every test moves with its code.
- **Tier 2 (integration):** unchanged. `make test-integration-rustd` is the
  cutover proof — the same 117-plus tests against the same live datastores.
- **Tier 3 (source-level):** per §2's Dimension 2.2, an assertion that a
  forbidden import path does not reappear.

## Acceptance Rubric (single scoring surface)

| # | Claim | Oracle | Bar | Priority | Result |
|---|-------|--------|-----|----------|--------|
| R1 | No behaviour changed | `make test-integration-rustd` | count and outcome identical to baseline | P0 | **Deferred to the M178 boundary** — the lane needs live datastores and runs once, at VERIFY, for both milestones together. |
| R2 | No coverage lost | `make test-unit-all` | count identical to baseline | P0 | **PASS**, with the delta accounted for in the baseline note: no test moved for a decomposition reason. |
| R3 | No cycle survives | `cargo build --workspace` | builds | P0 | **PASS** — and proved ahead of the compiler by Tarjan over `use crate::` statements at each step. |
| R4 | Each crate owns one error type | review | no crate matches another's variants | P1 | **PASS**, exceeded: the planes above compose with `#[from]` and DELEGATE `code()`/`detail()` rather than restating a mapping. |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the table | P0 | **AMENDED** — the table named `afd_lease` and `afd_vault`; what landed is `afd_credential` and `afd_gate`, for the reasons in §4 and Discovery. |
| R6 | The crate is actually smaller | `wc -l` | no crate over 12,000 lines | P1 | **PASS** — largest is `afd_fleet` at 8,044. |

## Dead Code Sweep

A decomposition surfaces dead code by making it fail to compile: a `pub(crate)`
item nobody outside its new crate uses becomes a warning the moment the boundary
moves. Every such warning is either a deletion or a missing `pub` — recorded,
not silenced.

## Out of Scope

- Any behaviour change, wire change or schema change.
- Splitting `afd_api`, which is 5,885 lines and not the problem.
- A Rust rewrite of `src/runner/`. That binary shares `afd_wire`,
  `afd_fleet_runtime` and `afd_core` with the daemon, and those are already
  separate crates — the daemon/runner boundary does not run through `afd_fleet`
  at all, so this milestone neither helps nor hinders it.

## Product Clarity (authoring record)

1. **Who is this for** — every engineer in the repository, on every build.
2. **What breaks if it is not done** — nothing breaks; everything stays slow,
   and the cost compounds as `afd_fleet` grows.
3. **Why now** — M178 established the pattern and proved the acyclic part. The
   cycles are documented while the measurement is fresh.
4. **Rebuild-vs-iterate** — iterate. No module is rewritten; calls are inverted.

## Decomposition & alternatives (patch vs refactor)

**Alternative considered: leave it.** `afd_fleet` works. The argument against is
compounding: every module added to it makes every future edit slower, and the
cycles get harder to break the more code sits on them.

**Alternative considered: one crate per module.** Twelve crates, each with an
error type, most under 2,000 lines. Rejected — `lease`, `gate`, `money` and
`policy` are one decision made in four files, and separating them would put a
crate boundary through the middle of the claim path.

## Discovery (consult log)

### The cycles were not what the graph said

The spec's graph is built from `crate::` occurrences. Five of the edges it puts
on cycles are **doc links** — `` [`Admission`](crate::lease::Admission) `` and
four like it — which rustdoc resolves and the compiler never sees. Counting
them made the crate look four times more tangled than it was, and it is the
reason four sections were budgeted for dependency inversion.

The graph that decides whether a crate can split is `use crate::` alone. Built
that way, the 17,763-line component came apart on four moves:

| what | why it was holding a cycle |
|---|---|
| `credential/mint.rs` → `lease/mint.rs` | 266 lines of `impl Plane` — an inherent impl on a type the crate does not own, which the ORPHAN RULE already required to live elsewhere. Misfiled, not entangled. |
| `sql/gate.rs`, `sql/grant.rs` → `gate/` | RULE SQLMOD. `gate` was the only reader; the edge dissolved rather than being cut. |
| `FIELD_REFRESH_TOKEN` → `afd_core::credential` | One RFC 6749 field name, written by one plane and read by the other. |
| the whole of `sql/` → its nine readers | The last shared node, and the reason the graph still had a floor after the cycles were gone. |

**No trait was introduced and no call was inverted.** An agent picking this up
should read the comprehension handshake's demand — "state which direction each
cycle gets inverted" — as a question worth answering with a measurement first,
because here the honest answer was "none of them".

### Metrics review, including the part that did not work

Clean `cargo build --workspace` before: **145.4s wall, 375.5s user**, with a
critical path of 43,814 serialized lines through
`core → crypto → auth → state → runner → afd_fleet(19,404) → afd_api → agentsfleetd`.
`afd_fleet` alone was 44% of that path.

After: **40,855 serialized lines — a 7% improvement, not the ~34% the split was
expected to buy.** The reason is worth recording, because it is the trap this
kind of refactor sets. The three crates are a CHAIN, not siblings:

```
  afd_credential(6351) → afd_gate(5388) → afd_fleet(8044)   = 19,783 serialized
  afd_fleet as one crate                                    = 19,404 serialized
```

A crate boundary buys parallelism only when the pieces can compile at the same
time. These cannot: a lease depends on a gate, which depends on policy, which
depends on a provider. That ordering is the domain, not an artifact of the
split, and no partition of these modules removes it.

**What the split actually bought is the incremental loop**, which is what a
developer feels: editing `lease` now recompiles 8,044 lines rather than 19,404,
and the same for each other plane in isolation. It also made
`afd_billing` and `afd_credential` independently linkable, which is the
precondition for the separate billing and vault binaries — that was Indy's
stated reason for the boundary, and it is met.

**A future milestone chasing clean-build time should target `afd_api` (9,336,
and the largest node left) or widen the L1 floor, not split further here.**
Splitting for count rather than width is what produced the 7%.

### Consults

- **Naming, Indy's call by implication.** The spec's `afd_vault` target name was
  taken by M178 §4 — the OPERATOR's secret surface, a different plane from the
  runner's credential reader. Landed as `afd_credential`; the two stay apart
  deliberately, and `afd_vault`'s own module note says why.
- **Branch, Indy (this session).** Folded into M178's branch rather than a
  worktree: *"i dont need a new worktree / it must be in this branch / that you
  are working on"*. The gate refuses two active specs per worktree, so this file
  stayed in `pending/` during the work and moves straight to `done/`.
- **Scope, Indy (this session).** *"i dont want you to adhere to the exact
  milestone of M184 fully but you have your memory here on how you have
  bifurcated the codebase"* — the partition below is the measured one, and it
  agrees with the spec's on everything but the two names.

### What the boundaries surfaced

A decomposition makes dead code fail to compile, and every case was decided
rather than silenced:

- `Vault::open`, `open_many`, `rotate_refresh_token` became `pub` — the
  operations a caller across the boundary actually invokes.
- `Vault::pool` and `Vault::kek` stayed module-private. Their doc says nothing
  outside `vault` may hold a connection that can UPDATE a credential; only the
  OPERATION crossed.
- `Declared::with_mintable` moved from `#[cfg(test)]` to the `test-util`
  feature (M-TEST-UTIL), because its caller is now another crate's test build.
- `Registry` gained a `SHIPPED` const: `#[non_exhaustive]` stops another crate
  naming the unit struct, and `Default::default()` returns a temporary a
  borrowed connector cannot outlive.
- Eight provider/vault error variants, their raisers, and nine dependencies
  (`base64`, `uuid`, `http`, `jsonwebtoken`, `moka`, `octocrab`, `reqwest`,
  `sha2`, `zeroize`) left `afd_fleet` with the code that used them.

### Deferrals

None. The one rubric row not answered here (R1, the integration lane) is not a
deferral — it runs once at the M178 boundary for both milestones, which is what
`docs/VERIFY_TIERS.md` prescribes.

### Declared divergences

- **The crate names differ from the plan.** `afd_lease` and `afd_vault` in the
  Files Changed table are `afd_gate` and `afd_credential` as built. `afd_vault`
  was unavailable — M178 §4 holds it — and `afd_gate` names what the crate
  actually decides (policy + the gate a run stops at) where `afd_lease` would
  have named the plane that stayed behind.

- **One sentence changed, and it was a defect being corrected.** Six crates
  created recently — `afd_approval`, `afd_billing`, `afd_credential`,
  `afd_events`, `afd_gate`, `afd_runner` — answered an unreachable datastore
  with an invented `"Service temporarily unavailable"`, where every PORTED
  crate answers `problem_response.zig`'s `"Database unavailable"`. One
  condition reaching a client as two different sentences depending on which
  plane it hit. Surfaced by `tenant_billing.rs` when the charges reader moved
  into `afd_billing` and its parity assertion started failing. All six now
  answer the ported sentence, and `afd_core::error::DETAIL_DATABASE_UNAVAILABLE`
  declares it once so a seventh crate cannot drift again. This is the milestone
  changing behaviour, which its own Invariant 1 forbids — recorded as a
  divergence rather than hidden, because the behaviour it changed was wrong.
