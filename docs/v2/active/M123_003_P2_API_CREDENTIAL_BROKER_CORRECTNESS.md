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

# M123_003: Persist rotated refresh tokens and key the broker cache on grant identity

**Prototype:** v2.0.0
**Milestone:** M123
**Workstream:** 003
**Date:** Jul 09, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — two bounded correctness gaps in the credential broker: a reconnected/rotated grant keeps serving the previously-minted token until its ≤1h expiry (self-healing, narrow window, needs an already-privileged operator), and a rotating-refresh provider (Jira) drops its rotated refresh token and forces a roughly-hourly reconnect. Neither is cross-tenant or unauthenticated.
**Categories:** API
**Batch:** B1 — independent; no shared files with other active workstreams.
**Branch:** feat/m123-broker-correctness
**Test Baseline:** unit=2402 integration=267
**Depends on:** None.
**Provenance:** agent-generated (pre-spec) — the Jul 09, 2026 `m122-gap-audit-security` workflow audited three credential-broker areas a Jul 02 coverage critic flagged and never reached. Both findings below passed an adversarial refutation pass (1/1 uphold each). A third finding — a missing single-flight guard blamed for refresh-token reuse revocation — was REFUTED: the real defect is rotation-persistence, not concurrency (see Discovery). Each finding re-verified against current source before drafting.
**Canonical architecture:** `docs/architecture/data_flow.md` — the mint/lease/grant gate chain the broker serves; `docs/architecture/runner_fleet.md` — the on-demand credential-mint path (`POST /v1/runners/me/credentials/mint`).

---

## Overview

**Goal (testable):** a broker mint whose vault handle changed identity (a github installation switch, a static Personal Access Token (PAT) rotation) re-reads the handle and mints a fresh token instead of serving the cached one, while a mint that changed only its rotating secret still hits the cache; and a mint against a rotating-refresh provider persists the rotated refresh token so the next cold mint succeeds instead of failing `invalid_grant`.

**Problem:** Two discarded signals in the broker. (1) `broker.zig` keys the token cache on only `(workspace, integration)` — the handle's identifying fields (github `installation_id`, the stored token) are never folded in. When an operator reconnects a workspace's github connector to a locked-down installation, the grant gate and vault load both pass on the new handle, but a cache hit within skew returns the OLD installation's higher-privilege token for up to its ~1h life — the intentional privilege reduction silently lags. Self-healing at token expiry, needs an already-privileged operator to trigger, bounded to one integration's cache entry. (2) `integration_oauth_refresh.zig` parses only `access_token`/`expires_in`; a rotated `refresh_token` in the provider response is parsed and dropped. The refresh token is written once at connect time and treated as static. For Atlassian three-legged OAuth (3LO) (Jira), which rotates by default, the stored token is invalidated on first refresh, so every subsequent cold mint re-posts the dead token → `invalid_grant` → the user is forced to reconnect roughly hourly. Zoho (permanent refresh tokens) and Linear are unaffected in steady state; the fix is provider-agnostic and a no-op for non-rotating providers.

**Solution summary:** Fold a fingerprint of the handle's stable identity fields — the whole handle MINUS the rotating-credential fields `refresh_token`, `access_token`, and `expires_at_ms` — into the cache key, so a reconnect that changes identity misses the cache while an ordinary refresh-token rotation does not thrash it. Have the oauth2-refresh mint capture a rotated `refresh_token` from the response, thread it through the broker's `ok` result, and persist it to the vault in the mint handler after a successful mint (non-fatal, logged on failure). Excluding the rotating fields from the fingerprint is the load-bearing reconciliation that lets both fixes coexist without re-minting every request for a rotating provider.

## PR Intent & comprehension handshake

- **PR title (eventual):** Key the broker cache on grant identity and persist rotated refresh tokens
- **Intent (one sentence):** a reconnected or rotated credential stops serving a stale token, and a rotating-refresh provider stops forcing hourly reconnects.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/credentials/broker.zig` — `mint` (cache-hit vs miss paths), `writeKey` (lines 195-201, the `(workspace, id)` key), and `cacheMinted`/`TokenVal`; §1 changes the key builder and §3 threads the rotated token through the `ok` return.
2. `src/agentsfleetd/credentials/integration_oauth_refresh.zig` — `parseAccess` (reads only `access_token`/`expires_in`); §2 adds the rotated-refresh capture here. The in-file test harness (`testing.FakeGitHub` as the token endpoint) is the pattern for the new negative tests.
3. `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` — `innerRunnerCredentialsMint`/`loadMintInputs`; the conn is released BEFORE `broker.mint` (never held across the network exchange). §3's write-back re-acquires a conn after the mint returns.
4. `src/agentsfleetd/http/handlers/connectors/oauth_refresh.zig` — `storeHandle` and the `RefreshTriple` field constants (`F_REFRESH_TOKEN` etc.); §3 reuses this module's store path and shares its field-name constants rather than re-declaring them (RULE UFS/TFX).
5. `src/agentsfleetd/credentials/integration.zig` — `Minted`/`Outcome`/`MintResult`; §2/§3 add the optional rotated-token field with a `= null` default so github/static construction sites compile unchanged.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/credentials/broker.zig` | EDIT | cache key folds a stable-identity fingerprint of the handle; the `ok` return threads a rotated refresh token from the miss path (null on a cache hit) |
| `src/agentsfleetd/credentials/broker_test.zig` | EDIT | reconnect-changes-identity → fresh mint; refresh-only change → cache hit; rotated token propagated on miss, null on hit; ownership/leak under `testing.allocator` |
| `src/agentsfleetd/credentials/integration.zig` | EDIT | `Minted` gains `rotated_refresh_token: ?[]const u8 = null`; shared rotating-field-name constants for the fingerprint exclusion set |
| `src/agentsfleetd/credentials/integration_oauth_refresh.zig` | EDIT | `RESP_FIELD_REFRESH_TOKEN` const; `parseAccess` returns the response's refresh token when present and different from the one posted; in-file mint tests extracted to the sibling test file (File & Function Length gate — the file was at 336/350) |
| `src/agentsfleetd/credentials/integration_oauth_refresh_test.zig` | ADD | the extracted oauth2-refresh mint suite + the new rotation-capture tests (amended at EXECUTE: FLL forced the extraction) |
| `src/agentsfleetd/tests.zig` | EDIT | one test-discovery line for the extracted test file (amended at EXECUTE) |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | after a successful mint with a rotated token, re-acquire a conn and write the updated handle back to the vault; warn-log on write-back failure (RULE OBS) |
| `src/agentsfleetd/http/handlers/connectors/oauth_refresh.zig` | EDIT | shared helper to merge a rotated refresh token into an existing handle and re-store it; export the rotating field-name constants for reuse |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_integration_test.zig` | EDIT | end-to-end write-back: a rotating-provider cold mint updates the vaulted refresh token; a non-rotating mint leaves it unchanged |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (the rotating-field-name set and the `RESP_FIELD_REFRESH_TOKEN` wire key are named constants shared across modules, never re-declared — TFX); **ECL** (a write-back failure is distinct from a mint failure and never collapses the successful mint); **OBS** (rotation-persisted and write-back-failed are observable branches → scoped log lines); **DFS** (the new `Minted` field is genuinely variant, non-null only for rotating mints — not a dead constant field); **OWN** (the rotated-token copy has exactly one free path per allocation); **CFG** (the fingerprint exclusion is one data-driven field set, not a per-integration branch).
- **`dispatch/write_zig.md`** — all edits are `*.zig`: multi-step `errdefer` on the new owned token copy, tagged-union `ok` payload, pg-drain on the write-back query, cross-compile both linux targets, `std.testing.allocator` leak proof on the new allocation.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — five Zig source files edited | cross-compile `x86_64-linux` + `aarch64-linux`; `make memleak` for the new token-copy ownership |
| PUB / Struct-Shape | yes — `Minted` gains a field; a new shared write-back helper + exported constants | keep `Minted` a plain result payload (default-`null` field, no new type); the write-back helper is a free fn in the existing `oauth_refresh` namespace (operations-over-value, conventional layout) — shape verdict recorded at PLAN |
| File & Function Length (≤350/≤50/≤70) | yes — `broker.zig` (221 lines) and the mint handler grow | fingerprint helper is a small private fn; if the mint handler's function nears the fn cap, extract the write-back into a named helper rather than inlining |
| UFS (repeated/semantic literals) | yes | `RESP_FIELD_REFRESH_TOKEN`, the rotating-field-name exclusion set, and any log-state strings as named constants shared across modules |
| UI Substitution / DESIGN TOKEN | no | no UI surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING + LIFECYCLE yes; ERROR REGISTRY + SCHEMA no | §3 logs the write-back branches (scoped `credential_mint` logger, RULE OBS); the rotated-token copy gets `errdefer`/one-owner handling; no new error code (the mint already succeeded — a write-back failure is logged, not a new wire error); no schema change |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/credentials/integration_github.zig` `mint` — the `Outcome.ok` construction the new default-`null` field must not disturb; alignment: github/static sites keep their two-field literal. Divergence: only the oauth2-refresh strategy sets the rotated field.
- **Reference:** `src/agentsfleetd/http/handlers/connectors/oauth_refresh.zig` `storeHandle` + the Jira/Linear/Zoho callbacks — the exact vault-store path and per-provider handle shape §3's write-back reuses (merge the rotated token into the same handle struct, re-store under the same provider). Divergence: none — the write-back stores through the identical helper the connect callback already uses.

## Sections (implementation slices)

### §1 — Cache key folds a stable-identity fingerprint — DONE

The token cache keys on `(workspace, integration)` alone, so a rewritten handle (reconnect to a different github installation, a rotated static PAT) is served the previously-minted token until skew. Fold a fingerprint of the handle's STABLE identity into the key: hash the handle object with the rotating-credential fields removed. **Implementation default:** a 64-bit non-cryptographic hash (`std.hash.Wyhash`) over the handle's identity fields in a canonical key order (sort keys so parser/insertion order can't change the fingerprint), excluding a named field set — `refresh_token`, `access_token`, and `expires_at_ms`; the fixed-width fingerprint is appended to the existing key buffer after the integration tag. Excluding the rotating fields is required so §2/§3's write-back does not force a cache miss on every request for a rotating provider (Decomposition). This fingerprint fix subsumes the secondary "bound the static cache below 24h" proposal — a rotated static PAT changes the fingerprint and re-mints immediately — so `MAX_TTL_S` is left unchanged.

- **Dimension 1.1** — two mints for the same `(workspace, integration)` whose handles differ in a NON-excluded identity field — a github `installation_id` switch AND a `static` handle whose `token` field changed (the rotated-PAT case) — each return that handle's own freshly-minted token; the second is NOT the first's cached token → Test `test_reconnect_identity_change_remints`
- **Dimension 1.2** — two mints whose handles differ ONLY in an excluded rotating field (`refresh_token`) share a cache hit (the second returns the cached token, no second exchange) → Test `test_refresh_only_change_hits_cache`
- **Dimension 1.3** — the fingerprint is order-independent: two handles with identical fields in different JSON key order produce the same cache key (one exchange across both mints) → Test `test_fingerprint_canonical_order`

### §2 — OAuth refresh mint captures a rotated refresh token — DONE

`parseAccess` reads only the access token and expiry; a rotated `refresh_token` in the response body is parsed into the JSON value and freed unread. Add the response-field constant and return the rotated token on the `ok` outcome when the body carries a `refresh_token` that differs from the one posted (a non-rotating provider that omits or echoes the same token yields `null` — no needless write-back). **Implementation default:** dedupe against the posted refresh token inside the strategy (it holds both values), so the handler stays simple; the returned token is duped with `ctx.alloc` and owned by the caller.

- **Dimension 2.1** — a refresh response carrying a NEW `refresh_token` returns it as `Minted.rotated_refresh_token` alongside the fresh access token → Test `test_refresh_response_rotates_token`
- **Dimension 2.2** — a refresh response that omits `refresh_token`, or echoes the posted one unchanged, returns `rotated_refresh_token = null` → Test `test_refresh_response_no_rotation`

### §3 — Broker threads it; the mint handler persists it — DONE

The broker returns `rotated_refresh_token` on `MintResult.ok` from the miss path (a cache hit did no exchange, so it stays `null`), owning/duping the copy for the caller and freeing the strategy's copy exactly once. The mint handler, after a successful mint carrying a rotated token, re-acquires a conn (the mint's conn was already released before the network exchange) and writes the updated handle back to the vault via the shared merge-and-store helper. A write-back failure is logged at warn and never fails the request — the child already holds a valid access token; the honest bound is that a crash in the sub-second persist window still costs one forced reconnect (Failure Modes / Discovery), reduced from "every ~1h, always."

- **Dimension 3.1** — a cold (miss-path) mint returns the rotated token on `MintResult.ok`; a cache-hit mint returns `null` → Test `test_broker_threads_rotated_on_miss_only`
- **Dimension 3.2** — the broker frees the strategy's rotated-token copy and hands the caller an independent copy (zero leak under `testing.allocator`, no double-free) → Test `test_broker_rotated_token_ownership`
- **Dimension 3.3** — end-to-end: a rotating-provider cold mint rewrites the vaulted handle's `refresh_token`; a subsequent cold mint uses the persisted token and succeeds (no `invalid_grant`) → Test `test_mint_persists_rotated_refresh_token`
- **Dimension 3.4** — a non-rotating (static/echo) mint leaves the vaulted handle byte-identical (no write-back) → Test `test_mint_no_rotation_leaves_handle_unchanged`
- **Dimension 3.5** — a write-back failure is logged at warn and the mint still returns 200 with the token → Test `test_write_back_failure_logged_not_fatal`

## Interfaces

```
broker.mint(alloc, workspace, integration_id, handle, now_ms) -> MintResult
  — signature unchanged; MintResult.ok gains rotated_refresh_token: ?[]const u8
    (null when no exchange occurred or the token did not rotate).

integration.Minted
  — gains rotated_refresh_token: ?[]const u8 = null  (default keeps github/
    static construction sites two-field). Owned by the constructor's allocator.

POST /v1/runners/me/credentials/mint
  — request/response wire shape UNCHANGED. The write-back is a server-side vault
    update; the 200 body still carries only { token, expires_at_ms }.
```

The cache-key layout is internal and exposes no wire shape; the vaulted handle shape is unchanged (the write-back rewrites the same field the connect callback wrote).

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reconnect to a different github installation | operator switches the connector in place | new handle → different identity fingerprint → cache miss → mint from the new installation; the stale higher-privilege token is never served |
| Static PAT rotated in the vault | operator re-stores a new token | the `static` handle's `token` field is non-excluded → fingerprint changes → immediate re-mint; no 24h staleness window (`test_reconnect_identity_change_remints`, static case) |
| Rotating provider returns a new refresh token | Atlassian 3LO refresh | rotated token threaded to the handler and persisted; next cold mint uses it and succeeds |
| Write-back fails (DB unavailable / conn acquire fails) | pool pressure at persist time | warn-logged; the request still returns 200 with the valid access token; next cold mint may force one reconnect |
| Crash between successful exchange and persist | daemon dies in the sub-second window | the rotated token is lost and the provider invalidated the old one → one forced reconnect (inherent to rotating-refresh OAuth; bound stated, not hidden) |
| Non-rotating provider (Zoho/static) mint | refresh token permanent or absent | `rotated_refresh_token = null`; no write-back; handle byte-identical |

## Invariants

1. A cache entry is served only when the handle's stable-identity fingerprint matches — enforced by folding the fingerprint into the cache key (a changed identity is structurally a different key, hence a miss), proven by `test_reconnect_identity_change_remints`.
2. The identity fingerprint excludes the rotating-credential field set, so an ordinary refresh-token rotation is a cache hit — enforced by the named exclusion set + `test_refresh_only_change_hits_cache`.
3. `rotated_refresh_token` is non-null only on a miss-path mint whose response actually rotated the token; a cache hit yields `null` — enforced by the broker returning the default on the hit path + `test_broker_threads_rotated_on_miss_only`.
4. A rotated refresh token returned by a successful mint is either persisted to the vault or its persist failure is warn-logged — never silently dropped — enforced by the handler write-back + RULE OBS grep + `test_write_back_failure_logged_not_fatal`.
5. The rotated-token copy has exactly one free path per allocation (broker frees the strategy copy; the caller frees its own) — enforced by `errdefer`/one-owner discipline + the zero-leak assertion in `test_broker_rotated_token_ownership`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `credential_mint.refresh_rotated` warn (failed) / debug (persisted) log | ops | §3 persists a rotated refresh token or its write-back fails | `workspace_id`, `integration`, outcome (persisted/failed), `err` name on failure | no token/refresh-token bytes in the log line (VLT) | `test_mint_persists_rotated_refresh_token`, `test_write_back_failure_logged_not_fatal` |

Amended at EXECUTE (spec-vs-rules): the authored "warn/info" pair violated `LOGGING_STANDARD.md` §10A.L4 (the `info` allow-list is fixed) — the persisted branch logs at `debug`. The failure `warn` carries no `error_code` per §5/§100: the mint itself succeeded, no wire error maps to the best-effort persist failure, and the domain consequence surfaces on a later mint as the already-registered `UZ-CONN-006`.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_reconnect_identity_change_remints` | github: mint(handle A: installation X) then mint(handle B: installation Y, same ws+integration) → second returns Y's token, not X's cached token; static: mint(handle token=T0) then mint(handle token=T1) → second re-mints, not T0's cached token (both handle shapes exercised) |
| 1.2 | unit | `test_refresh_only_change_hits_cache` | mint(handle refresh=R0) then mint(handle refresh=R1, else identical) → second is a cache hit; exactly one exchange |
| 1.3 | unit | `test_fingerprint_canonical_order` | two handles, same fields, different JSON key order → one shared cache key (one exchange) |
| 2.1 | unit | `test_refresh_response_rotates_token` | fake endpoint returns a new `refresh_token` → `Minted.rotated_refresh_token` = that value, access token still fresh |
| 2.2 | unit | `test_refresh_response_no_rotation` | response omits `refresh_token` OR echoes the posted one → `rotated_refresh_token == null` |
| 3.1 | unit | `test_broker_threads_rotated_on_miss_only` | miss-path mint → rotated token on `ok`; cache-hit mint → `null` |
| 3.2 | unit | `test_broker_rotated_token_ownership` | mint under `testing.allocator` with a rotating fake strategy → zero leaks, no double-free; caller copy independent of the freed strategy copy |
| 3.3 | integration | `test_mint_persists_rotated_refresh_token` | rotating fake provider + vaulted jira handle → cold mint rewrites the handle's `refresh_token`; a second cold mint uses it, returns 200 (no `invalid_grant`) |
| 3.4 | integration | `test_mint_no_rotation_leaves_handle_unchanged` | static/echo handle → cold mint leaves the vaulted handle byte-identical |
| 3.5 | integration | `test_write_back_failure_logged_not_fatal` | write-back path forced to fail → 200 with the token still returned; warn log observed |

Regression: existing broker tests (`test_...caches a token within validity`, `...re-mints past the skew`, the oauth2_refresh mint suite) must stay green — the fingerprint and the default-`null` field must not change their outcomes. Idempotency: a repeated cold mint with the SAME (already-persisted) rotated token performs no needless write-back (Dimension 3.4 covers the null-rotation case).

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Reconnect/rotation of identity re-mints; refresh-only change hits cache (§1) | `make test-unit-all` | exit 0 incl. the three §1 tests | P0 | ✅ all four sub-lanes exit 0 (`test depth gate passed (unit=2414 integration=270)`) |
| R2 | Rotated refresh token captured + persisted (§2/§3) | `make test-integration` | exit 0 incl. `test_mint_persists_rotated_refresh_token` | P0 | ✅ exit 0; execution proven by sabotage canary going red in this lane |
| R3 | Non-rotating provider unaffected — handle unchanged, no write-back (§3) | `make test-integration` | exit 0 incl. `test_mint_no_rotation_leaves_handle_unchanged` | P0 | ✅ exit 0 (updated_at sentinel unchanged) |
| R4 | No token/refresh bytes logged in the write-back branches | `grep -n "refresh_rotated" src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | log line names only `workspace_id`/`integration`/outcome/`err`, no token arg | P0 | ✅ lines 142/145 carry exactly those fields |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ 9 code paths = amended table; +the spec file itself |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ agentsfleetd/runner/lib/coverage each exit 0 (`Ran 1305 tests across 143 files`) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `All lint checks passed` |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ exit 0 (`Full integration suite passed`); isolated-stack re-run `2028 pass, 10 skip` — see Session Notes on shared-stack interference |
| S5 | No leaks (broker token-copy ownership touched) | `make memleak` | exit 0 | P0 | ✅ `1541 passed; 498 skipped; 0 failed` |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ both targets exit 0 |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (153.24 MB scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted. No public symbol renamed; the new `Minted` field and the shared constants are additive.

## Out of Scope

- A single-flight / in-flight-mint guard on the broker — the refuted third finding blamed it for refresh-token reuse revocation; the real defect is rotation-persistence (this spec). A single-flight guard would at most delay revocation by one mint cycle, not prevent it. Not added here.
- The residual lease-expiry-during-exchange race already documented in `credentials_mint.zig` — a separate atomic-mint follow-up, untouched.
- Dropping Jira from `REGISTRY` — the finding's alternative (B); rejected because Jira is a genuinely shipped connector, and the write-back fix supports it correctly rather than removing it.
- Changing `MAX_TTL_S` / the cache TTL policy — the fingerprint fix subsumes the secondary static-staleness proposal.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reconnects a workspace's github connector to a locked-down installation and the very next tool call runs under the reduced-privilege token, not the old one; a Jira user connects once and never sees a surprise "reconnect required" later in the session.
2. **Preserved user behaviour** — every happy-path mint (github, static, Zoho, Linear, and Jira within a valid token's life) returns the same token shape it does today; only the failure/rotation branches change.
3. **Optimal-way check** — folding a fingerprint into the existing key and threading one optional field is the most direct fix; the write-back reuses the connect callback's own vault-store path. No new endpoint, no new error code, no schema change.
4. **Rebuild-vs-iterate** — iterate: two contained correctness gaps on an otherwise-correct broker; both mirror existing patterns (data-driven key, tagged `ok` payload, shared store helper). No redesign, no determinism traded away.
5. **What we build** — one fingerprinted cache key, one rotated-token capture, one threaded result field, one non-fatal vault write-back.
6. **What we do NOT build** — a single-flight guard, an atomic mint+persist transaction, a Jira removal, or a TTL-policy change (each in Out of Scope).
7. **Fit with existing features** — compounds the grant-gate/lease scope already enforced in the mint handler; must not destabilize the cache's sharded-concurrency behavior or the static/github mint outcomes.
8. **Surface order** — API-only internal hardening; no CLI/UI surface (the mint path is runner-to-daemon). The user-visible effect is the absence of a spurious reconnect and the presence of prompt privilege reduction.
9. **Dashboard restraint** — the only new signal is a scoped log line on rotation-persist/failure; no counter or UI control is added until the log shows the branch is exercised in production.
10. **Confused-user next step** — a Jira user who still hits a reconnect after this ships sees the existing `reconnect_required` copy (`UZ-CONN-006`); the write-back's warn log is the operator's self-serve breadcrumb for a persist failure.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — the cache-key fingerprint (finding 1), the strategy-layer rotation capture, and the broker-thread-plus-handler-persist (finding 2 split by layer). The exclusion of the rotating fields from §1's fingerprint is the load-bearing reconciliation: without it, §3's write-back would change the handle on every rotation and force a cache miss per request for rotating providers.
- **Alternatives considered:** (a) fingerprint the WHOLE handle including the refresh token — rejected: defeats the cache for rotating providers, one exchange per request, needless rotation churn and rate-limit exposure; (b) drop Jira from `REGISTRY` and document a non-rotating-only assumption — rejected: Jira is shipped, and removing a working connector to dodge a persistence bug is not honest; (c) add a single-flight guard (the refuted finding's frame) — rejected: rotation-persistence, not concurrency, is the root cause, and single-flight only delays revocation by one cycle.
- **Patch-vs-refactor verdict:** this is a **patch** — two discarded-signal gaps closed by mirroring patterns already in the file (data-driven key, shared store helper, tagged `ok` payload); the only structural addition is one optional result field with a default.

## Discovery (consult log)

- **Consults** — Architecture: no consult needed — the cache-key layout is broker-internal; neither `data_flow.md` nor `runner_fleet.md` describes it, and the mint flow shape is unchanged. Legacy-Design: none triggered. Gate-flag triage (mechanical, auto-applied): UFS flagged four inline `3600`s in the new broker tests → extracted to shared response-body consts; LENGTH forced the oauth2-refresh test extraction (Files Changed amended).
- **Refuted-finding record** — a third audit finding claimed a missing single-flight guard caused refresh-token reuse detection to revoke the token family. The refuter killed it: for a genuinely rotating provider the design is already broken SEQUENTIALLY because the rotated token is never persisted; single-flight would delay revocation by one mint cycle, not prevent it. Root cause = rotation-persistence (§2/§3); the concurrency framing was a red herring. No single-flight added.
- **Provider rotation scope (established from code)** — Zoho refresh tokens are permanent (unaffected); Jira/Atlassian 3LO rotates by default (the confirmed break); Linear returns refresh pairs (`linear/callback.zig`) but is not asserted to rotate on refresh — the fix is provider-agnostic (persist-if-rotated), correct for whichever provider rotates and a no-op for those that do not, so scope is pinned to behavior, not a provider allow-list.
- **Severity honesty** — both findings verified P2 (1/1 uphold each): finding 1 is a bounded least-privilege lag (self-healing at ≤1h expiry, needs an already-privileged operator, single cache entry — not cross-tenant/unauth); finding 2 forces a roughly-hourly reconnect for a rotating provider only. Neither is a live cross-tenant or unauthenticated exploit.
- **Metrics review** — one scoped log line added (`credential_mint.refresh_rotated`); no analytics/funnel playbook applies to backend credential plumbing.
- **Skill-chain outcomes** — `/write-unit-test`: diff ledger 11/11 resolved (8 tested, 3 won't-test with reasons: private fingerprint helpers proven through the public mint surface; shared constants compile-checked; handler extraction is behavior-preserving line moves). Audit added two tests beyond the spec's ten: the writeKey key-buffer-overflow fail-closed path, and an exhaustive FailingAllocator sweep over the oauth2_refresh mint (every allocation index, zero leaks). Mutation probes 2/2 killed doubling as red-green proof: fingerprint neutered to a constant → `test_reconnect_identity_change_remints` red; rotation capture forced null → `test_broker_threads_rotated_on_miss_only` red + rotation test crash. Test Delta vs baseline: unit 2402→2414 (+12), integration 267→270 (+3).
- **Deferrals** — empty at creation..
