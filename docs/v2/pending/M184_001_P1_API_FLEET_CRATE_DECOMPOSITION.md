# M184_001: A crate you cannot split by moving files is a crate with a cycle in it

**Prototype:** v2.0.0
**Milestone:** M184
**Workstream:** 001
**Date:** Aug 27, 2026
**Status:** PENDING
**Priority:** P1 — no behaviour changes, and every build in the repository pays for this one until it lands
**Categories:** API
**Batch:** B8 — NOT parallel with anything touching `afd_fleet`; see Dependencies
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M178_001 — hard, and for two reasons. Its §3–§6 add handlers over `vault`, `secrets`, `gate` and `sweep`, which are the modules this spec restructures; and it CREATES `afd_tenant`, so a branch taken from `main` before it merges would be decomposing a crate graph that no longer exists.
**Provenance:** LLM-drafted (Claude Opus 5, Aug 27, 2026)
**Canonical architecture:** `docs/architecture/direction.md` §Two daemons, one contract

---

## Overview

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
| `rustd/crates/afd_lease/**` | CREATE | the claim-and-spend plane: lease, gate, money, policy — what a runner may run next and what it may spend doing it |
| `rustd/crates/afd_vault/**` | CREATE | sealed material and what opens it: vault, secrets, provider, credential broker |
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

### §1 — The free three

`sweep`, `runner` and `bundle` depend on nothing. They move first, alone, so the
mechanical half of the work is landed and reviewable before any cycle is
touched.

- **Dimension 1.1** — the three modules move with their SQL and their tests, and
  the workspace still builds → Test: `make test-unit-all` count unchanged.

### §2 — Invert `lease ↔ credential`

`lease` calls the broker to mint a credential for a claim; `credential` reads
the lease to scope what it mints. The inversion candidate is the second
direction: what `credential` needs is not the lease STORE but the lease's
identity and reach, which is a value.

- **Dimension 2.1** — `credential` takes a resolved scope value rather than
  reaching for the lease store → Test: the mint suite, unchanged in count.
- **Dimension 2.2** — no `crate::lease` path remains under `credential/` → Test:
  a source-level assertion, the way M178's route-inventory test pins its roster.

### §3 — Invert `lease ↔ gate ↔ policy ↔ money`

The claim path's own cluster. These four are one plane and are candidates to
stay together in `afd_lease` rather than be separated — in which case the only
cycle to break is the one crossing OUT of the group.

- **Dimension 3.1** — the four move as one crate and the edges leaving it are
  one-directional → Test: `cargo tree` shows no cycle; the build is the proof.

### §4 — Invert `provider ↔ vault ↔ secrets ↔ credential`

The sealed-material cluster, same question as §3: do these four separate, or are
they one crate with clean edges out?

- **Dimension 4.1** — the group compiles as `afd_vault` with one-directional
  edges → Test: the vault and broker suites, unchanged in count.

### §5 — One error type per crate

Each new crate gets its own, composed at the seams with `#[from]`, and
`afd_api::handler::Refusable` gains an impl per plane.

- **Dimension 5.1** — no crate matches on another's error variants → Test: the
  refusal-envelope suite still renders every code it rendered before.

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

## Invariants

1. **Behaviour is unchanged.** Every route answers the same status, code and
   sentence before and after.
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
| R1 | No behaviour changed | `make test-integration-rustd` | count and outcome identical to baseline | P0 | |
| R2 | No coverage lost | `make test-unit-all` | count identical to baseline | P0 | |
| R3 | No cycle survives | `cargo build --workspace` | builds | P0 | |
| R4 | Each crate owns one error type | review | no crate matches another's variants | P1 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the table | P0 | |
| R6 | The crate is actually smaller | `wc -l` | no crate over 12,000 lines | P1 | |

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

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question
  asked + Indy's decision.
- **Metrics review** — `cargo build --timings` before and after.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`,
  `orly-babysit-prs` results.
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim
  quote, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

### Declared divergences

None yet. This milestone changes no behaviour, so a divergence here would be a
defect rather than a decision.
