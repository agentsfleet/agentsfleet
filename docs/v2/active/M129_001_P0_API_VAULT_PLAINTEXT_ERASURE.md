<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M129_001: Erase Vault Plaintext After Use

**Prototype:** v2.0.0
**Milestone:** M129
**Workstream:** 001
**Date:** Jul 13, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — secret-bearing request and response storage currently survives request completion without deterministic erasure
**Categories:** API
**Batch:** B1 — isolated security hardening
**Branch:** feat/m129-plaintext-erasure
**Test Baseline:** unit=2585 integration=311
**Depends on:** M123_001 — envelope encryption and transient key zeroing are the established vault baseline
**Provenance:** agent-drafted (Codex, Jul 13, 2026) from an adversarial Fable review independently checked against Zig 0.16 and httpz source
**Canonical architecture:** `docs/architecture/data_flow.md` §C and `docs/architecture/billing_and_provider_keys.md` §8.2

---

## Overview

**Goal (testable):** Secret-bearing vault, request, dispatch-arena, and lease/mint response bytes are deterministically zeroed after their final consumer, including parse failures and partial HTTP writes.
**Problem:** Envelope encryption protects database rows, but decrypted plaintext, canonical secret JSON, request bodies, parser allocations, and serialized lease or mint responses can remain in reusable process memory after a request completes. The current code has no proven dangling-slice defect; this work narrows the lifetime and residue of valid plaintext copies.
**Solution summary:** Add zero-before-free at plaintext choke points, erase secret-bearing request bodies after borrowed typed values are destroyed, back each dispatch arena with an allocator that wipes complete allocations on release, and synchronously write then erase sensitive lease and mint response buffers. Any sensitive response write failure closes the HTTP connection so a truncated response cannot be followed by another response on the same connection.

## PR Intent & comprehension handshake

- **PR title (eventual):** Erase vault plaintext after request use
- **Intent (one sentence):** Reduce the value of process-memory disclosure by ensuring secret plaintext does not remain in reusable application buffers after vault, lease, or credential-mint work finishes.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/secrets/crypto_store.zig` — established zero-before-free ordering for key material on success and error paths.
2. `src/agentsfleetd/http/server.zig` and the pinned httpz `response.zig` / `worker.zig` sources — dispatch-arena ownership, response-buffer retention, and write-failure handover behavior.
3. `docs/v2/done/M123_001_P1_API_VAULT_ENVELOPE_CRYPTO_HARDENING.md` — prior vault threat model and failure-injection expectations.
4. `dispatch/write_zig.md` allocator rules 1–6 — Ghostty-derived arena ownership and allocation-failure discipline.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/secrets/zeroizing_allocator.zig` | CREATE | Backing allocator that wipes released allocations without destructive speculative shrink behavior |
| `src/agentsfleetd/secrets/zeroizing_allocator_test.zig` | CREATE | Prove free, shrink fallback, remap fallback, and allocation-failure behavior |
| `src/agentsfleetd/state/vault.zig` | EDIT | Zero raw decrypted plaintext before release on every exit |
| `src/agentsfleetd/state/vault_test.zig` | EDIT | Prove load success and parse failure release zeroed plaintext storage |
| `src/agentsfleetd/http/server.zig` | EDIT | Back the dispatch arena with the zeroizing allocator |
| `src/agentsfleetd/http/handlers/common.zig` | EDIT | Add one sensitive JSON write boundary with exact capacity, synchronous write, failure close, and buffer erasure |
| `src/agentsfleetd/http/handlers/hx.zig` | EDIT | Expose the sensitive response operation through the existing handler context |
| `src/agentsfleetd/http/handlers/hx_test.zig` | EDIT | Prove sensitive response routing and error behavior through the handler context |
| `src/agentsfleetd/http/handlers/fleets/secrets.zig` | EDIT | Zero store/rotate plaintext and request bodies in correct lifetime order |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | Zero the request body and use the sensitive response boundary for minted tokens |
| `src/agentsfleetd/fleet/service.zig` | EDIT | Use the sensitive response boundary for secret-bearing leases |
| `src/agentsfleetd/tests.zig` | EDIT | Import the new unit-test module into the canonical test root |
| `docs/architecture/data_flow.md` | EDIT | Record dispatch and serialized-response erasure at the lease boundary |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Correct the provider-key memory boundary to include response-buffer erasure |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — OWN, NDC, NLR, UFS, FLL, TST-NAM, XCC, VLT, TST, ZIG, ESO, HXX, GRD.
- **`dispatch/write_zig.md`** — allocator lifecycle, `errdefer`, ownership comments, public-surface shape, failure injection, and both Linux cross-compiles.
- **`dispatch/write_any.md`** — file/function limits, logging review, no milestone identifiers in source, and unified literals.
- **`docs/AUTH.md`** — runner credential mint remains lease-bound and no authorization semantics change.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — existing routes and wire shapes remain unchanged.
- **`dispatch/name_architecture.md`** — the existing lease and credential flows are updated at their canonical documentation homes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — Zig memory ownership changes | format, unit/integration tests, leak gate, and both Linux cross-compiles |
| PUB / Struct-Shape | yes — one reusable allocator type and one handler operation | operations-over-value modules with no unused public surface |
| File & Function Length (≤350/≤50/≤70) | yes | new focused modules; split common response logic before either touched file crosses its cap |
| UFS (repeated/semantic literals) | yes | import existing constants; new semantic strings declared once |
| UI Substitution / DESIGN TOKEN | no — no TypeScript or user interface change | not applicable |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | lifecycle only | no new response outcome, error code, log payload, or schema change; allocator ownership is single-shot and failure-tested |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/secrets/crypto_store.zig` — mirror its deferred `secureZero` then free sequence.
- **Reference:** `~/Projects/oss/ghostty/src/config/Config.zig` — preserve arena-as-owner teardown; erasure is an `agentsfleet` extension, not a Ghostty behavior claim.
- **Reference:** Zig 0.16 `std.mem.Allocator.VTable` and `std.heap.ArenaAllocator` — shrinking and remapping must fall back rather than zero still-live bytes.

## Sections (implementation slices)

### §1 — Plaintext choke-point erasure

Raw decrypted bytes, canonical store/rotate JSON, and secret-bearing request bodies are erased after their final consumer. Typed request parses may borrow body slices, so parsed values must be destroyed before body erasure.

- **Dimension 1.1** — vault plaintext is zeroed before free on successful and failed JSON parse → Test `vault load releases zeroed plaintext on success and parse failure`
- **Dimension 1.2** — store and rotate canonical plaintext is zeroed before free on successful and failed persistence → Test `secret writes release zeroed canonical plaintext`
- **Dimension 1.3** — store, rotate, and credential-mint request bodies are erased only after borrowed parsed values are destroyed → Test `secret request bodies remain valid through parse cleanup then erase`

### §2 — Dispatch-arena erasure

Every dispatch arena returns all pages through a zeroizing backing allocator, covering reachable values, parser growth garbage, duplicate-key allocations, numeric token storage, and partial parse failures. **Implementation default:** shrinking and remapping are refused so the caller performs allocate-copy-free; speculative tail erasure is forbidden because a failed shrink would corrupt a live allocation.

- **Dimension 2.1** — freeing an allocation zeroes its full initialized region before the child sees the free → Test `zeroizing allocator wipes complete allocation before free`
- **Dimension 2.2** — shrink and remap requests cannot erase live bytes when the child cannot perform them safely → Test `zeroizing allocator preserves live allocation when shrink or remap falls back`
- **Dimension 2.3** — dispatch teardown wipes every arena page on normal and allocation-failure exits → Test `dispatch arena releases only zeroed pages across allocation failures`

### §3 — Sensitive response erasure

Lease and credential-mint success responses serialize without abandoned response blocks, write synchronously, and erase the serialized bytes after the socket has consumed them. A write error marks the connection for close before returning. Ordinary non-secret responses keep the worker-driven path.

- **Dimension 3.1** — lease success uses the sensitive writer and leaves serialized provider and secret bytes zeroed → Test `lease response buffer is zero after synchronous write`
- **Dimension 3.2** — credential-mint success uses the sensitive writer and leaves the minted token buffer zeroed → Test `mint response buffer is zero after synchronous write`
- **Dimension 3.3** — partial or failed sensitive write closes the connection and never permits worker reuse → Test `sensitive response write failure closes connection`
- **Dimension 3.4** — exact preallocation prevents abandoned secret-bearing response blocks → Test `sensitive response serialization performs one body allocation`

### §4 — Architecture and regression proof

Canonical architecture states exactly which copies are erased and which remain outside this guarantee. Existing HTTP routes, JSON shapes, authorization, and successful lease/mint behavior remain unchanged.

- **Dimension 4.1** — architecture names raw plaintext, request body, dispatch arena, and serialized response boundaries without claiming headers or active-use memory are erased → Test `plaintext erasure architecture names all four boundaries`
- **Dimension 4.2** — existing lease and mint integration behavior remains wire-compatible → Test `sensitive response path preserves lease and mint JSON`

## Interfaces

```
ZeroizingAllocator.init(child_allocator) -> allocator whose released storage is zeroed.
Hx.okSensitive(status, body) -> existing JSON envelope and wire shape, synchronously written and erased.

Existing routes, request fields, response fields, status codes, authorization checks, and database schema remain unchanged.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Vault JSON parse failure | decrypted plaintext is malformed | return the existing parse error after plaintext and partial arena storage are erased |
| Store or rotation failure | persistence rejects canonical plaintext | return the existing error after canonical and request buffers are erased |
| Dispatch allocation failure | any arena allocation returns Out of Memory | existing internal error behavior; every allocated page is erased during unwind |
| Sensitive serialization failure | exact capacity or JSON serialization fails | existing internal error response when still writable; partial buffer is erased |
| Sensitive socket write failure | peer closes or write is partial then errors | connection handover becomes close; truncated response is never followed by another response |
| Allocator shrink fallback | child cannot resize in place | no live bytes are modified; caller allocates, copies, then zero-frees the old allocation |

## Invariants

1. No memory is erased before its final borrower is destroyed — enforced by defer ordering and request-body lifetime tests.
2. Every dispatch-arena page reaches a zero-before-free operation on success and error exits — enforced by a capture allocator and exhaustive allocation failure injection.
3. A failed allocator shrink or remap never modifies the caller's live allocation — enforced by rejecting destructive operations and testing preserved bytes.
4. A sensitive response is either fully written or its connection is closed — enforced by the failed-write handover test.
5. Sensitive serialization produces no abandoned secret-bearing growth allocation — enforced by an allocation-count assertion.
6. HTTP and JSON interfaces are unchanged — enforced by existing integration tests and the new wire regression test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | erasure occurs during existing cleanup | none | no secret values, sizes, or token fragments are logged | `sensitive response path preserves lease and mint JSON` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit, negative | `vault load releases zeroed plaintext on success and parse failure` | capture allocator observes zeroed decrypted storage for valid and malformed JSON |
| 1.2 | integration, negative | `secret writes release zeroed canonical plaintext` | successful and injected-failure store/rotate paths release only zeroed canonical buffers |
| 1.3 | unit, negative | `secret request bodies remain valid through parse cleanup then erase` | borrowed fields are readable until parsed cleanup; full body becomes zero afterward |
| 2.1 | unit | `zeroizing allocator wipes complete allocation before free` | initialized secret bytes are zero when the child allocator receives free |
| 2.2 | unit, negative | `zeroizing allocator preserves live allocation when shrink or remap falls back` | fallback returns failure/null and every original byte remains unchanged until later free |
| 2.3 | unit, negative | `dispatch arena releases only zeroed pages across allocation failures` | exhaustive injected allocation failures leak zero bytes and capture only zeroed frees |
| 3.1 | integration | `lease response buffer is zero after synchronous write` | secret map and provider key serialize to the expected body, then buffered bytes are zero |
| 3.2 | integration | `mint response buffer is zero after synchronous write` | minted token reaches the client once, then buffered bytes are zero |
| 3.3 | integration, negative | `sensitive response write failure closes connection` | deterministic failed write sets close handover and a second response cannot be appended |
| 3.4 | unit, invariant | `sensitive response serialization performs one body allocation` | exact sizing yields one response-body allocation and no growth allocation |
| 4.1 | unit, source assertion | `plaintext erasure architecture names all four boundaries` | both canonical documents name vault, request, dispatch arena, and serialized response erasure limits |
| 4.2 | integration, regression | `sensitive response path preserves lease and mint JSON` | real routes retain existing status and response fields for successful lease and mint requests |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Plaintext and dispatch storage erase on success and failure (§1–§2) | `make test-unit-agentsfleetd` | exit 0 including zeroizing allocator and plaintext lifetime tests | P0 | |
| R2 | Sensitive responses erase and failed writes close (§3) | `make test-integration-fast` | exit 0 including lease/mint response and failed-write tests | P0 | |
| R3 | Architecture names the exact erasure boundary (§4) | `rg -n "zero|erase" docs/architecture/data_flow.md docs/architecture/billing_and_provider_keys.md` | matches for vault plaintext, request body, dispatch arena, and serialized response | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit and integration tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | No leaks | `make memleak` | exit 0 with 0 leaks | P0 | |
| S4 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S5 | No secrets committed | `gitleaks detect` | exit 0 | P0 | |
| S6 | Source files remain within limits | `git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted and no public symbols renamed or removed.

## Out of Scope

- Erasing Authorization-header bytes or the entire httpz connection read buffer; body erasure does not claim complete request erasure.
- Preventing a compromised running process, debugger, kernel, or hypervisor from reading plaintext during active use.
- Changing runner-token lifetime, capability scopes, tenant API-key policy, vault encryption, or managed key-service ownership.
- Shutdown-only erasure of process-lifetime configuration secrets; request-lifetime residue is the higher-value boundary here.

## Product Clarity (authoring record)

1. **Successful user moment** — a security test captures every released secret-bearing request allocation as zeros while the existing lease and mint clients still receive valid responses.
2. **Preserved user behaviour** — secret store, rotate, lease, and credential-mint calls retain their routes, authentication, status codes, and JSON bodies.
3. **Optimal-way check** — framework-owned request headers remain outside the guarantee; the selected boundary covers application-owned plaintext and the reusable body/response buffers proven to contain secrets.
4. **Rebuild-vs-iterate** — iterate on the existing arena and handler ownership model; replacing httpz or the request dispatcher would add risk without improving the four verified choke points.
5. **What we build** — zero-before-free cleanup, one zeroizing allocator, body cleanup at three handlers, one sensitive response operation, tests, and two architecture corrections.
6. **What we do NOT build** — recursive JSON walking, a no-op parse allocation option, a new endpoint, schema changes, or broad framework replacement.
7. **Fit with existing features** — extends M123 key zeroing and M102 lease-bound minting; must not destabilize keepalive correctness or wire compatibility.
8. **Surface order** — API-only internal hardening; no command-line or user-interface surface changes.
9. **Dashboard restraint** — N/A — no user-visible control or security claim is added.
10. **Confused-user next step** — N/A — failures preserve existing typed API errors; no new remediation action exists.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections mirror four ownership boundaries: plaintext producers, dispatch storage, serialized responses, and durable architecture proof.
- **Alternatives considered:** recursive JSON walking was rejected because parser garbage and error-path allocations are unreachable; wiping after speculative shrink was rejected because a failed shrink leaves corrupted live memory; replacing httpz was rejected because one isolated sensitive writer can enforce the required boundary.
- **Patch-vs-refactor verdict:** this is a **refactor** because deterministic erasure belongs at allocation and response-lifecycle boundaries rather than as scattered value-tree cleanup.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: Fable adversarial review corrected the false dangling-slice claim and identified retained response storage; Indy approved implementing the corrected four-boundary design on Jul 13, 2026.
- **Metrics review** — no analytics or funnel signal changes; erasure emits no values or size metadata.
- **Skill-chain outcomes** — `/write-unit-test`: pending; `/review`: pending; `kishore-babysit-prs`: pending after push.
- **Deferrals** — none.
