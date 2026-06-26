# Zig engineering idioms — the house style we're converging on

> Parent: [`README.md`](./README.md)
>
> **Scope:** durable design direction, not a binding guarantee. Captures the
> Bun (`oss/bun/src`) Zig idioms we are adopting as house style for registries,
> strategies, and wire types, plus the sequencing that keeps the
> security-sensitive, semi-frozen `/v1/runners` wire safe. First instance landed
> in M102_001 §4 (the `integration.Mint` strategy union + the typed, out-of-band
> `ExecutionPolicy.mintable` list).

## Thesis

We already write these patterns ad hoc — `MintCtx` is dependency injection, the
integration registry is a declarative table, `Outcome`/`MintResult` are typed
result unions. The goal is to make the style **systematic**, so the *Nth*
integration / provider / webhook is a **data line**, not an engineering project.
That directly de-risks the connector roadmap (github → jira, zoho, posthog,
grafana, zoho-recruit, …).

The patterns below were mined from Bun's Zig codebase; each cites the precedent.

## The bets, ranked by return

### 1 — Strategy unions for every "kind + behavior" pair
*Precedent: Bun `SideEffects` (`resolver/package_json.zig`), `AllowUnresolved` (`options.zig`).*

A tagged union over the *strategy*: declarative-data variants for the common
cases + a function-pointer escape hatch for the bespoke one, with the union
owning its own dispatch method (so callers never branch on the id).

Landed: `integration.Mint = union(enum){ static, custom: *const fn(MintCtx)… }`
with `Mint.run` / `Mint.isOnDemand`. The broker calls `spec.mint.run(ctx)` — no
per-id branch (RULE CFG / Invariant 4).

Next candidates: the §5 webhook `PROVIDER_REGISTRY` verifier schemes
(`hmac_sha256_body`, `hmac_sha256_ts_body`, `atlassian_jwt`) → a `Scheme` union
with the same shape; provider-credential resolution. The declarative
`oauth2_refresh` variant for refresh-token providers (Zoho, Jira) slots into
`Mint` as DATA when its first real caller lands (M103) — **not before** (untested
dead code, RULE NDC).

### 2 — Comptime registries with comptime validation
*Precedent: Bun `ComptimeStringMap` (`comptime_string_map.zig`, length-bucketed, benchmarked faster than a switch); `EnumFields`/`std.meta` dispatch; CSS codegen.*

We hand-roll several registries (`integration.REGISTRY`,
`tool_bridge.BRIDGE_REGISTRY` — a linear scan over 33 tools, the error registry,
the webhook registry) each with bespoke comptime checks. Standardize on a
comptime-map helper that gives O(1) lookup **and** compile-time exhaustiveness
(every id has a spec; every `UZ-` code unique; no mintable-but-unregistered
integration). Turns a class of "forgot to register / duplicate id" bugs into
compile errors. Sequenced for M102_002.

### 3 — Pure core + injected effects as the default, not the exception
*Precedent: Bun s3 `SignOptions → SignResult` (`s3/credentials.zig`), a pure function testable with no HTTP; our `MintCtx`, our `run_context.RunDeps` seam.*

Highest *immediate* payoff: half of §3/§4's tests are DB-gated and can't run
locally because logic is fused to Postgres/network. The cure is the `MintCtx`
idiom already validated — push it through `secrets_resolve`, the broker handler,
billing: pure decision-core, effects passed in. Makes the integration tier
unit-testable instead of CI-only.

### 4 — Typed result unions over stringly codes + sentinels
*Precedent: Bun `S3StatResult = union(enum){ success, not_found, failure }`.*

The spec requires "every Error Table row → a negative test." A typed outcome
union per operation makes that **compiler-enforced** (exhaustive switch), not
test-enforced. We are halfway — `Outcome`/`MintResult` are good; HTTP handlers
still hand-map stringly `UZ-` codes.

### 5 — Generate dispatch from one source of truth *(aspirational)*
*Precedent: Bun `css/properties_generated.zig` — 800 properties + parse/serialize/eql/hash from one comptime table.*

The error registry, wire types, and provider tables *could* be generated from
one declarative source. High payoff at scale, speculative for our size. Flagged,
not proposed.

## What we deliberately do NOT copy from Bun

- **No `anyopaque` / manual vtables** (Bun's `ImportWatcher`). Our type system is
  a *security asset* at the sandbox boundary; type erasure there is a regression.
- **Borrow idioms, not architecture.** Bun is one process with shared globals and
  aggressive allocator tricks. We are a daemon/runner split with a semi-frozen
  wire and a hard isolation boundary; some Bun conveniences *conflict* with our
  invariants. Take the patterns, not the posture.
- **No abstracting at N=1.** Bun's `SideEffects` grew to 5 variants over years; we
  deferred `oauth2_refresh` until Zoho is a real second caller. Discipline holds.

## Sequencing (no big-bang)

| When | Bet | Why there |
|---|---|---|
| M102_001 §4 (landed) | 1 + 3 (pilot) | typed `mintable` + `Mint` union on a small, owned surface |
| M102_002 (concurrency) | 2 | comptime registries for broker + `tool_bridge`; resolve-once ties into mint concurrency |
| M103 (more integrations) | 1 (full) + 4 | `oauth2_refresh` variant + webhook `Scheme` union (N≥2 validates the abstraction) |
| backlog | 5 | only if registry count keeps growing |

## Bottom line

The win is not elegance — it is (1) every future connector collapses from "a
file + a function" to "a data line," and (2) the DB-gated test problem is solved
by the same DI idiom we already trust. We ride milestones, behind existing seams,
never a rewrite.
