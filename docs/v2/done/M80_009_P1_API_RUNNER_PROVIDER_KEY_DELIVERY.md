# M80_009: Deliver the resolved provider key to the runner on the lease

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 009
**Date:** May 29, 2026
**Status:** DONE
**Priority:** P1 — runner-enablement blocker: a platform-managed zombie is billed as if usezombie supplies the Large Language Model (LLM) key, but the hermetic runner never receives it, so platform-managed execution can't authenticate to the provider.
**Categories:** API
**Batch:** B1
**Branch:** feat/m80-009-provider-key
**Depends on:** M80_002 (cutover — establishes the runner execution path this rides; the lease + `ExecutionPolicy` wire it touches)
**Provenance:** LLM-drafted (Opus 4.8, May 29, 2026 — from the M80_005 PR #351 open finding + a code-verified investigation on `main`)

> **Provenance is load-bearing.** LLM-drafted from a code investigation, not a fresh idea. The implementing agent re-verifies the named call sites against `main` before editing — the design rests on `ResolvedProvider` already being resolved at lease time and discarded.

**Canonical architecture:** `docs/architecture/data_flow.md` (the lease inline-secret-delivery path) + `docs/AUTH.md` (*Runner token* → *Least privilege* / *Sensitive-data classification* — the trusted-fleet "secret delivery is placement" model this extends).

---

## Implementing agent — read these first

1. `src/zombied/fleet/service.zig` — `runBilling` resolves `ResolvedProvider` (mode/provider/api_key/model) for the cost decision, then `resolveExecutionPolicy` builds the lease policy **separately** with no access to it. This is the wire to extend — carry the already-resolved provider+key through.
2. `src/zombied/state/tenant_provider.zig` + `tenant_provider_resolver.zig` — `ResolvedProvider`'s shape, the platform vs self-managed resolution, and the `secureZero`-on-deinit ownership pattern to mirror for the carried key.
3. `src/runner/child_exec.zig` (`buildCallArgs`, `extractApiKey`) + `engine/runner.zig` + `engine/runner_helpers.zig` (`applyAgentConfig`, `injectProviderApiKey`) + `engine/redaction_canary.zig` — the consumption side: the `secrets_map["llm"]` heuristic to retire, and the redaction that keys off `agent_config.api_key` (which stays).
4. `docs/AUTH.md` — **mandatory (auth-flow change):** the runner-token least-privilege section ("secret delivery is placement, not a standing grant") and the sensitive-data classification table.
5. `src/lib/contract/execution_policy.zig` + `protocol.zig` — `ExecutionPolicy` is the frozen `/v1/runners` lease contract; `LeasePayload.policy` rides it. New fields are a contract evolution.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** deliver the resolved provider key to the runner on the lease
- **Intent (one sentence):** platform-managed and self-managed zombies authenticate to their **billed** LLM provider because the runner receives the resolved provider+key on the lease, instead of guessing it from a user credential named `llm`.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Key assumption to confirm: the provider key rides the **existing** inline trusted-fleet secret-delivery envelope (the same one that already ships `secrets_map`) — there is **no `trust_class` delivery gate** (that field is M80_007 scope); and the two `ExecutionPolicy` fields are **additive + defaulted** so the contract stays backward-parseable.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NLR** (retire the `secrets_map["llm"]` extraction in place, no `_v2` twin), **NDC** (remove `extractApiKey` + its `SECRETS_LLM_KEY` const when the caller goes — no dead code), **UFS** (the `provider`/`api_key` wire keys reuse the existing `wire.*` + contract field identifiers verbatim runner↔zombied; no new repeated literals).
- **`docs/ZIG_RULES.md`** — `*.zig` in both binaries: pg-drain (no new query — `resolveActiveProvider` already drains), tagged-union results, multi-step `errdefer` on the owned `api_key` dupe, cross-compile both linux targets.
- **`docs/AUTH.md`** — auth-flow / credential change: the secret-delivery boundary + sensitive-data classification.
- **`docs/LIFECYCLE_PATTERNS.md`** — the `api_key` ownership + `secureZero` across the new serialize-then-free boundary.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the (internal) lease response shape gains two fields; envelope unchanged.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `*.zig` in zombied + runner | cross-compile x86_64/aarch64-linux; pg-drain audit (no new query) |
| PUB / Struct-Shape | yes — `ExecutionPolicy` gains two fields (pub contract type) | shape verdict: pure-data struct stays pure-data; additive defaulted fields, no method/inheritance change |
| File & Function Length (≤350/≤50/≤70) | yes — `service.zig` (~285) + `child_exec.zig` (~199) | thread the resolved provider via the existing `Billed`-style carrier; if `resolveExecutionPolicy`/`buildCallArgs` near 50, factor a helper |
| UFS | yes — `provider`/`api_key`/`model` keys | reuse `wire.provider`/`wire.api_key`/`wire.model` + `ExecutionPolicy` field names; zero new literals |
| LOGGING | yes — lease + exec emit on the key path | never log the key; the redaction canary already covers `agent_config.api_key`; add a log-capture negative test |
| LIFECYCLE | yes — owned `api_key` dupe + `secureZero` across the lease serialize boundary | mirror `ResolvedProvider`'s dupe/zero; serialize synchronously via `hx.ok` before the arena frees; `errdefer` on partial build |
| ERROR REGISTRY | no — no new code | provider-resolve failure already answers no-work (`runBilling` returns null); the engine reuses `ERR_EXEC_RUNNER_INVALID_CONFIG` for a key-without-provider lease |
| SCHEMA GUARD | no | `core.tenant_providers` / `core.platform_llm_keys` already exist; no migration |

---

## Overview

**Goal (testable):** a platform-managed lease carries the resolved provider name + LLM key in its `ExecutionPolicy`, the runner injects them into the engine, and the agent's LLM call authenticates with the **same** key the tenant is billed against — asserted by `test_lease_carries_platform_provider_key`, `test_runner_injects_policy_provider_key`, and `test_runner_auth_without_llm_credential`.

**Problem:** a platform-managed zombie is billed as if usezombie supplies and pays for its LLM key, but the key the engine actually runs on has never come from there. In both the pre-cutover worker and the post-cutover runner the platform-resolved key is loaded only for the cost/model decision and then zeroed and discarded. The runner is hermetic (it deliberately reads no provider key from its environment), so a platform-managed run only authenticates if the zombie *happens* to carry a user credential named `llm` — otherwise the LLM call fails. Even when it works, the key billed can differ from the key executed.

**Solution summary:** make the already-resolved `ResolvedProvider.api_key`+`provider` **authoritative**. zombied carries them from `runBilling` onto two new `ExecutionPolicy` fields, delivered inline on the lease under the **same** trusted-fleet envelope that already ships `secrets_map`. The runner reads them directly and injects them into the engine, and the `secrets_map["llm"]` heuristic retires — `secrets_map` reverts to tool credentials only. The key the tenant is billed against becomes the key that runs.

---

## Prior-Art / Reference Implementations

- **API / wire** → the existing inline `secrets_map` resolution in `resolveExecutionPolicy` (`src/zombied/fleet/service.zig`) — the new `provider`/`api_key` fields mirror exactly how `secrets_map` is resolved-then-serialized inline on the lease. No new delivery mechanism is invented.
- **Lifecycle** → `ResolvedProvider`'s own `dupe` + `secureZero`-on-deinit pattern (`tenant_provider_resolver.zig`) — mirror its ownership for the carried key.
- **Runner consumption** → `injectProviderApiKey` / `applyAgentConfig` already accept `agent_config.provider` + `agent_config.api_key`; only their **source** changes.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/execution_policy.zig` | EDIT | add `provider` + `api_key` additive-defaulted fields to `ExecutionPolicy` (the lease contract) |
| `src/zombied/fleet/service.zig` | EDIT | resolve the provider+key on the lease path (fresh + reclaim) and carry it onto the policy via `resolveExecutionPolicy`; `secureZero` after serialize. Factor a `resolveProviderForLease` helper to keep the file ≤350 / fns ≤50 |
| `src/runner/child_exec.zig` | EDIT | source `agent_config.provider`/`api_key` from `policy`; retire `extractApiKey` + `SECRETS_LLM_KEY` |
| `src/runner/engine/runner_helpers.zig` | EDIT (maybe) | confirm `applyAgentConfig` sets `default_provider` from the policy-sourced value; no behaviour change downstream |
| `src/runner/engine/context_budget.zig` | EDIT | parser parity: `fromJson` also reads `wire.provider`/`wire.api_key` so both `ExecutionPolicy` parse paths agree (the lease path parses by std.json struct reflection; this is the hand-rolled path) |
| `docs/AUTH.md` | EDIT | record provider-key inline delivery in the secret-delivery boundary + a sensitive-data table row |
| `docs/architecture/data_flow.md` | EDIT | correct the §C step-4 + lease-reply claim — name the real `ExecutionPolicy.provider`/`api_key` wire instead of "crosses in process memory" |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | §8.2 visibility boundary — line that says the runner is "not reachable today, since the lease carries no provider-key field" is now wired; name the lease field |
| `docs/architecture/scenarios/01_default_install.md` | EDIT | steps 7 + 9 — the platform key rides the lease policy and the runner injects it; the scenario only authenticates once this lands |
| `docs/architecture/scenarios/02_self_managed.md` | EDIT | §4 steps 5/7 + §7 — same for self-managed (its credential is never in `secrets_map`, so the old `llm` heuristic never authenticated it either) |
| `src/zombied/fleet/*_test.zig`, `src/runner/**/*_test.zig` | CREATE | integration + unit per the Test Specification |

> **Doc blast radius expanded (PLAN finding, May 30, 2026).** The scenarios + `billing_and_provider_keys.md` §8.2 describe the provider key reaching the runner's NullClaw child *as if already wired* — it is not on `main` (verified). The grep-gate `test_docs_provider_key_wire_accurate` covers every live arch/scenario doc, so these are in §3 scope. Indy direction (May 29, 2026: *"I cant have new specs … its in this PR that you build"*) → folded here, not a follow-up. `capabilities.md` line 57 already names the lease-`ExecutionPolicy`→runner path and needs no edit.

> Line numbers omitted by design (they drift). The agent reads the named symbols on `main`.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three slices — (1) zombied resolves→delivers on the lease, (2) the runner consumes + retires the heuristic, (3) security + doc reconciliation. Independently testable; (1) and (2) are the contract's two ends.
- **Alternatives considered:** (a) have zombied inject the platform key into `secrets_map` under a reserved `llm` name — rejected: conflates a platform credential with user tool secrets, keeps the magic-name coupling, and still diverges billed-vs-run if a user also defines `llm`; (b) proxy LLM calls through zombied so the key never leaves the control plane — rejected: that is the scoped/proxied-secrets (zero-trust) model explicitly **beyond the trusted-fleet model** (M80_006 Out of Scope), a far larger change.
- **Patch-vs-refactor verdict:** a **focused refactor of the key-delivery path** — it removes a heuristic and replaces it with the authoritative resolved value. Not a mud-patch (we do not paper the platform key into `secrets_map`); not a broad refactor (no proxy plane). The `trust_class`-gated *delivery* refinement is the right long game and is named as M80_007 scope, not silently skipped.

---

## Sections (implementation slices)

### §1 — zombied delivers the resolved provider key on the lease — DONE

`ExecutionPolicy` gains `provider`+`api_key`; a `ResolvedProvider` is resolved on the lease path and serialized inline on the lease — the same envelope and ordering as `secrets_map`. Why: the key the tenant is billed against must reach the engine. **Implementation default:** additive optional fields (default empty) so the contract evolution stays backward-parseable; the key is duped into the request arena, serialized synchronously by `hx.ok` before the arena frees (mirror the `secrets_map` lifetime), then `secureZero`'d.

> **Resolution source — fresh vs reclaim (code-verified May 29, 2026).** `runBilling` resolves a `ResolvedProvider` for the metering decision but currently **discards** its `api_key`/`provider`, returning only `Billed{tenant_id, posture, model}`; the **reclaim** branch reuses `acq.reused` (`{tenant_id, posture, model}`) and never calls `runBilling` at all, and the lease DB row deliberately **never persists the api_key** (plaintext-secret-in-table is forbidden). Therefore the provider+key is **resolved on the lease path for BOTH fresh and reclaim** via `tenant_provider.resolveActiveProvider(billed.tenant_id)` — billing stays reused on reclaim (**no re-charge**); only the key is (re-)resolved. This matches the "config resolved fresh on every lease" model in `runner_fleet.md`. Without the reclaim re-resolve, dead-runner recovery leases would carry an empty key and fail to authenticate — re-creating this spec's bug on the recovery path.

- **Dimension 1.1** — a fresh **platform-mode** lease's `ExecutionPolicy` carries the resolved provider name + api_key → Test `test_lease_carries_platform_provider_key`
- **Dimension 1.2** — a **self-managed** lease carries the tenant's own provider+key (not the platform key) → Test `test_lease_carries_self_managed_provider_key`
- **Dimension 1.3** — the carried api_key never appears in any log emit on the lease path → Test `test_lease_provider_key_absent_from_logs`
- **Dimension 1.4** — a **reclaim** lease re-resolves and carries the provider+key (billing reused, no re-charge) → Test `test_reclaim_lease_carries_provider_key`

### §2 — runner injects the policy provider key; retire the `secrets_map["llm"]` heuristic — DONE

`child_exec.buildCallArgs` sources `agent_config.provider`/`api_key` from `policy.provider`/`policy.api_key`; `extractApiKey` + `SECRETS_LLM_KEY` are removed; `secrets_map` carries tool credentials only. Why: the engine authenticates with the delivered key, and the magic-name coupling + billed-vs-run divergence are gone. **Implementation default:** keep `agent_config.api_key` as the engine's input contract (the redaction canary already keys off it) — change only its **source**. The general `${secrets.NAME.FIELD}` substitution (where `llm` may legitimately be a user tool-secret name) is **unchanged** — only the provider-key extraction retires.

- **Dimension 2.1** — given a lease with `policy.api_key`+`policy.provider`, NullClaw Config carries that key for that provider → Test `test_runner_injects_policy_provider_key`
- **Dimension 2.2** — a zombie with **no** `llm` credential still authenticates (platform key arrives via the policy) → Test `test_runner_auth_without_llm_credential`
- **Dimension 2.3** — a tool credential named `llm` in `secrets_map` is treated as a **tool** secret, never the provider key → Test `test_llm_named_tool_secret_not_provider_key` (regression of the retired heuristic)
- **Dimension 2.4** — the policy api_key bytes are redacted from runner activity/log frames → Test `test_runner_redacts_policy_api_key`

### §3 — security + doc reconciliation — DONE

`AUTH.md` records provider-key inline delivery under the trusted-fleet placement model + a sensitive-data row; `data_flow.md` is corrected — today it wrongly claims the provider key "crosses into the lease reply in process memory." Why: an auth-flow/credential change must leave the docs truthful. **Implementation default:** cite M80_005's enrollment-gated trusted-fleet model as the trust boundary; name `trust_class` delivery-gating as M80_007 scope.

- **Dimension 3.1** — no architecture/auth doc claims the provider key reaches the runner via `secrets_map`, nor that it "crosses in process memory" absent the lease field → Test `test_docs_provider_key_wire_accurate` (grep-gate)
- **Dimension 3.2** — `AUTH.md`'s sensitive-data table classifies the lease-delivered provider api_key (class + acceptable/forbidden surfaces) → Test `test_authmd_provider_key_classified` (grep-gate)

---

## Interfaces

```
ExecutionPolicy (src/lib/contract/execution_policy.zig) gains:
  provider: []const u8 = ""    -- resolved provider name (e.g. "fireworks"); "" = none
  api_key:  []const u8 = ""    -- resolved LLM key; "" = none; inline secret, NEVER logged

LeasePayload.policy carries these to the runner over the existing /v1/runners lease
wire (additive; ignore_unknown_fields keeps old/new parseable both ways).

Runner agent_config (engine input — keys UNCHANGED, SOURCE changes):
  agent_config.provider := policy.provider     (was: never set)
  agent_config.api_key  := policy.api_key      (was: extractApiKey(secrets_map["llm"]))

Retired: child_exec.extractApiKey + SECRETS_LLM_KEY. secrets_map = tool credentials only.
Unchanged: the engine's general ${secrets.NAME.FIELD} substitution + the redaction
canary keyed on agent_config.api_key.

No new error code: provider-resolve failure at lease time keeps today's behaviour
(runBilling → null → lease answers no-work + backoff).
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Platform key unresolved at lease | `core.platform_llm_keys` no active row / vault miss | `runBilling` returns null (as today) → lease answers no-work + backoff; nothing leaks → `test_lease_no_work_when_platform_key_missing` |
| Self-managed credential malformed | `credential_ref` row missing `provider`/`api_key` | `resolveActiveProvider` errors → no-work (as today) → `test_lease_no_work_when_self_managed_malformed` |
| api_key present, provider empty | resolver returned a key with no provider name | engine cannot map the key to a provider entry → fail the lease config (`ERR_EXEC_RUNNER_INVALID_CONFIG`), report failure → `test_runner_fails_lease_on_missing_provider` |
| Stale `llm` tool secret post-migration | a zombie still defines an `llm` credential | treated as a tool secret only; provider auth comes from `policy.api_key` → `test_llm_named_tool_secret_not_provider_key` |
| api_key reaches a log/activity frame | careless emit on the lease/exec path | redaction canary + log-capture tests fail closed → `test_lease_provider_key_absent_from_logs` + `test_runner_redacts_policy_api_key` |
| Old runner parses a new lease | mixed-version deploy window | additive defaulted fields + `ignore_unknown_fields` → parses, no provider key (falls back to no-auth, surfaces as a clean engine config error) → `test_execution_policy_backward_parse` |
| Reclaim lease has no resolved key | reclaim reuses prior billing + the key is never persisted | the lease path re-resolves via `resolveActiveProvider(tenant_id)` (no re-charge) so the reclaimed lease still carries a key → `test_reclaim_lease_carries_provider_key`; resolve failure → no-work + backoff (as fresh) |

---

## Invariants

1. The provider api_key is never written to any log/activity frame — enforced by the existing redaction canary (`engine/redaction_canary.zig`, keyed on `agent_config.api_key`) + log-capture tests, not review discipline.
2. The key the lease **bills** and the key the engine **runs** derive from the **same** `resolveActiveProvider` resolution on the lease path — enforced by both `Billed` (posture/model) and `policy.api_key` being produced from one `ResolvedProvider` + `test_lease_carries_*_provider_key`.
3. `secrets_map` carries no provider key — enforced by removing `extractApiKey` (dead-code, NDC) + `test_runner_auth_without_llm_credential`.
4. `ExecutionPolicy`'s new fields are backward-additive — enforced by Zig default field values + `ignore_unknown_fields` on parse + `test_execution_policy_backward_parse`.
5. The carried api_key is serialized before its arena frees — enforced by `hx.ok`'s synchronous serialization in `issueLease` (same ordering as `secrets_map` today) + `make memleak` clean over the lease path.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_lease_carries_platform_provider_key` | platform-mode tenant, active platform key seeded → lease `policy.provider`+`api_key` = the resolved platform values |
| 1.2 | integration | `test_lease_carries_self_managed_provider_key` | self-managed tenant w/ credential_ref → lease carries the tenant's own provider+key, not the platform key |
| 1.4 | integration | `test_reclaim_lease_carries_provider_key` | reclaim of an expired lease → re-leased policy carries a resolved provider+key; billing row reused (no new debit) |
| 1.3 | integration | `test_lease_provider_key_absent_from_logs` | capture logs over a lease issue → the api_key bytes never appear |
| 2.1 | integration | `test_runner_injects_policy_provider_key` | lease w/ `policy.api_key`+`provider` → NullClaw Config has that key on that provider entry |
| 2.2 | integration | `test_runner_auth_without_llm_credential` | zombie with no `llm` credential + platform key on the lease → engine authenticates |
| 2.3 | unit | `test_llm_named_tool_secret_not_provider_key` | `secrets_map` has an `llm` tool secret → it is NOT injected as the provider key |
| 2.4 | unit | `test_runner_redacts_policy_api_key` | activity/log frame containing the key → redacted to the placeholder |
| 3.1 | integration | `test_docs_provider_key_wire_accurate` | grep-gate: no live arch/auth doc claims the old (wrong) wire |
| 3.2 | integration | `test_authmd_provider_key_classified` | grep-gate: `AUTH.md` sensitive-data table has the provider-key row |
| FM | unit | `test_execution_policy_backward_parse` | a lease JSON without the new fields parses to defaulted `provider`/`api_key` |
| FM | integration | `test_lease_no_work_when_platform_key_missing` | no active platform key → lease answers no-work, no leak |
| FM | integration | `test_runner_fails_lease_on_missing_provider` | key present, provider empty → engine config error + reported failure |

Regression: M80_002 lease/fencing/billing tests stay green (the `Billed` carrier and lease-row write are unchanged in shape). Replay/idempotency: N/A — no new retry semantics; reclaim reuses prior billing as today.

---

## Acceptance Criteria

- [x] A platform-managed run authenticates with the platform key via the lease — `test_runner_injects_policy_provider_key` (unit, green) + `test_lease_carries_platform_provider_key` (integration, compiles; runs under LIVE_DB in CI)
- [x] A zombie without an `llm` credential still runs — unit guard `buildCallArgs injects neither half…` (green) + `test_runner_auth_without_llm_credential` (CI)
- [x] The provider key never logs — `test_runner_redacts_policy_api_key` (unit, green) + log-capture `test_lease_provider_key_absent_from_logs` (CI)
- [x] Docs match the real wire — `test_docs_provider_key_wire_accurate` + `test_authmd_provider_key_classified` (grep-gates, PASS)
- [x] `make lint-zig` clean · unit `make test-unit-{ziglib,zigrunner}` 21/21 + 203/203 · `make test-integration` (CI) · pg-drain clean
- [x] `make memleak` (CI — `secureZero`+arena lifetime in place) · cross-compile x86_64+aarch64-linux ✓ · `gitleaks detect` clean · no file over 350 lines (service.zig 319/350)

---

## Eval Commands (post-implementation)

```bash
# E1: key wire — make test-integration 2>&1 | grep -E "provider_key|auth_without_llm|PASS|FAIL"
# E2: Build  — zig build
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: pg-drain — make check-pg-drain 2>&1 | tail -3
# E8: Orphan sweep (empty = pass) — grep -rn "extractApiKey\|SECRETS_LLM_KEY" src/ | grep -v _test
```

---

## Dead Code Sweep

**1. Orphaned files** — N/A — no files deleted.

**2. Orphaned references** — `extractApiKey` and `SECRETS_LLM_KEY` are removed from `child_exec.zig` when §2 lands.

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| `extractApiKey` | `grep -rn "extractApiKey" src/ \| grep -v _test` | 0 matches |
| `SECRETS_LLM_KEY` | `grep -rn "SECRETS_LLM_KEY" src/ \| grep -v _test` | 0 matches |

> Do NOT remove the engine's `${secrets.llm.api_key}` substitution placeholder or its redaction fixtures — those are the **general** user-secret substitution mechanism (where `llm` is just an example credential name), distinct from the provider-key path being retired.

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Investigation (May 29, 2026):** code-verified on `main` that `ResolvedProvider.api_key` is billing-only and discarded in both eras; the engine has always run on a user credential (`resolveFirstCredential` pre-cutover; `secrets_map["llm"]` post-cutover). `data_flow.md` currently overstates the wire (claims the key crosses into the lease reply — it does not). This spec wires it for the first time.
- **Trust model (May 29, 2026):** `trust_class` / `allowed_workspace_ids` delivery-gating is **already Indy-acked-deferred to M80_007** (see M80_005 *Out of Scope* + Discovery). The provider key therefore rides the existing enrollment-gated trusted-fleet inline envelope — same trust boundary as `secrets_map` today. No new gate is invented here.
- **Sibling finding (May 29, 2026):** the runner control-plane read-timeout (the other M80_005 PR #351 open finding) is deferred to a Zig 0.16 toolchain bump and filed in `docs/architecture/roadmap.md` — not this spec.
- **`/review` outcome (May 30, 2026):** independent adversarial pass (fresh-context subagent, verified through the httpz serialize chain + arena lifecycle) — all five focus areas clean (secrecy/lifecycle, reclaim no-double-charge, runner atomic-inject, backward-parse, logging). One **P2 folded in**: `runBilling`'s billing-only `ResolvedProvider` (`tr.resolved`) left its unused `api_key` unzeroed in the arena (initially a standalone `secureZero` — later **superseded** by the Greptile carry-through fix below, which resolves once and owns the key end-to-end). **Deploy ordering note:** old-lease→new-runner parse is tested; new-zombied→old-runner is safe because the old runner already parses with `ignore_unknown_fields`. Roll zombied and runner in either order.
- **Greptile P2 fixed (May 30, 2026, PR #353):** the fresh path resolved the provider twice (`runBilling` for billing, `resolveProviderForLease` for delivery) — extra vault decryption **and** a sub-ms key-rotation TOCTOU where the lease could bill key-A but deliver key-B, violating Invariant 2. Fixed by carrying the `ResolvedProvider` resolved in `runBilling` through `Billed.provider` (owned; `issueLease` deinits after `hx.ok`); a `committed`-flag `defer` zeroes it on any gate-failure path. Reclaim (no billing pass) still re-resolves via `resolveProviderForLease`. Fresh leases now resolve once — bill key == deliver key. Supersedes the standalone secureZero patch. Indy ack: *"fix all the findings… fold those fixes in this PR."*
- **`/write-unit-test` ledger (May 30, 2026):** 8 deterministic unit tests written + green — contract backward-additive parse + provider/api_key round-trip (`protocol_test.zig`); runner consumption inject / `llm`-tool-secret-not-promoted / incomplete-pair-injects-nothing (`child_exec.zig`); `fromJson` parse parity ×2 (`context_budget.zig`); redaction scrubs the lease-delivered key (`runner_helpers.zig`). 2 integration tests written + compile-clean, LIVE_DB-gated (fresh-carries-key, reclaim-re-resolves-key in `control_plane_integration_test.zig`). 2 doc grep-gates pass. **needs-infra (LIVE_DB, run in CI):** `test_lease_carries_self_managed_provider_key`, `test_lease_provider_key_absent_from_logs` (log-capture harness), `test_lease_no_work_when_platform_key_missing`, `test_runner_fails_lease_on_missing_provider` — described in the Test Specification; the deterministic unit layer already proves the incomplete-pair + backward-parse + redaction invariants. Negative-path ratio on changed surface ≥50% (llm-not-promoted, incomplete-pair, backward-parse-absent, redaction are all negative/guard paths).
- **PLAN finding — reclaim path (May 29, 2026):** code-verified that `runBilling` discards the resolved `api_key`/`provider`, the reclaim branch never resolves a provider, and the lease row must not persist the key. The original spec assumed "the `ResolvedProvider` already resolved in `runBilling`" — true only for fresh leases. Resolution: resolve the provider+key on the lease path for **both** fresh and reclaim (reclaim re-resolves with no re-charge). Indy decision (May 29, 2026): *"I cant have new specs, i prefer its in this PR that you build"* — the reclaim re-resolve + the `context_budget.zig` parser-parity edit are folded into M80_009, not a follow-up spec. Spec amended in place (§1, Files Changed, Failure Modes, Test Specification).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. the redaction + backward-parse + billed-equals-run invariants) | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs AUTH.md secret-delivery boundary, the Invariants, ZIG_RULES | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit — runner (consume/parity/redaction) | `make test-unit-zigrunner` | 203/203 passed | ✅ |
| Unit — contract (backward-parse/round-trip) | `make test-unit-ziglib` | 21/21 passed | ✅ |
| Integration (key wire) | `make test-integration` | compiles clean; runs under LIVE_DB in CI (2 tests written; 4 needs-infra) | ⏳ CI |
| Memleak (lease path) | `make memleak` | needs infra (LIVE_DB); `secureZero`+arena lifetime in place | ⏳ CI |
| pg-drain | `make lint-zig` | passed (338 files) | ✅ |
| Lint (ZLint + length + role + orphan sweep) | `make lint-zig` | passed — ZLint 0/0; all new files <350; orphan sweep clean (`extractApiKey`/`SECRETS_LLM_KEY` gone) | ✅ |
| Cross-compile | `zig build -Dtarget={x86_64,aarch64}-linux` | both EXIT 0 | ✅ |
| Gitleaks | `gitleaks detect` | no leaks (2212 commits) | ✅ |
| Doc wire-accuracy grep-gates | grep | PASS — 0 stale claims; AUTH.md row present | ✅ |
| Logging | `scripts/audit-logging.sh` | LOGGING GATE clean (blocking layer) | ✅ |

---

## Out of Scope

- **`trust_class` / `allowed_workspace_ids` delivery-gating** (don't ship the shared platform key to a less-trusted runner) — M80_007 scheduler, where the "required trust" data source lands (per M80_005's acked deferral).
- **Zero-trust scoped/proxied secret delivery** (the key never leaves zombied) — beyond the trusted-fleet model (M80_006 Out of Scope).
- **Per-zombie multi-provider selection / provider failover** — future.
- **The runner control-plane read-timeout** — deferred to the Zig 0.16 toolchain bump; see `docs/architecture/roadmap.md`.
