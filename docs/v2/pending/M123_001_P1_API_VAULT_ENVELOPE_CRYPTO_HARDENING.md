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

# M123_001: Bind vault ciphertext to its identity, zero key material, and test the envelope

**Prototype:** v2.0.0
**Milestone:** M123
**Workstream:** 001
**Date:** Jul 09, 2026
**Status:** PENDING
**Priority:** P1 — hardens the credential-vault security boundary (envelope-at-rest encryption). The verifiers deflated every finding to P2/P3 for one honest reason: exploiting the identity-binding gap presupposes an actor who already holds write access to `vault.secrets` — which the `api_runtime` role has (`schema/002_vault_schema.sql:70`). That makes this defense-in-depth, not a live exploit; the P1 is the boundary it protects, not a reachable attack.
**Categories:** API
**Batch:** B1 — runs alone; touches only the `secrets/` crypto module and its callers via unchanged public signatures.
**Branch:** {added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** None.
**Provenance:** agent-generated (pre-spec) — the Jul 09, 2026 `m122-gap-audit-security` workflow audited three areas a Jul 02 coverage critic flagged and never reached; every finding survived an adversarial refutation pass (finding 1: 3/3 uphold, verifier-corrected P1→P2; findings 2/3: 1/1 uphold P2; finding 4: 1/1 uphold P3). Each re-verified against current source before drafting.
**Canonical architecture:** `docs/AUTH.md` §Sensitive-data classification — the LLM-provider-key / vault-secret credential boundary this spec must not weaken; `docs/architecture/billing_and_provider_keys.md` — where resolved secrets are consumed.

---

## Overview

**Goal (testable):** every new `vault.secrets` envelope authenticates against its own `(workspace_id, key_name, kek_version)` — a row whose crypto columns are relocated to another key fails `DecryptFailed`; every plaintext key buffer (Data Encryption Key (DEK), Key Encryption Key (KEK) stack copies, unwrapped DEK, decoded master key) is `secureZero`'d before its storage is released on success and error paths; and the envelope lifecycle plus its three error branches and nonce-uniqueness carry direct tests.

**Problem:** `crypto_primitives.encrypt`/`decrypt` pass an empty string as the Authenticated Encryption with Associated Data (AEAD) associated data on both the DEK-wrap and the payload legs (`crypto_primitives.zig:89`, `:110`), so neither blob is cryptographically bound to the row identity, and the process-wide KEK (`g_kek`) is identical for every row. An actor who already holds a write foothold on `vault.secrets` (the `api_runtime` grant, or a write-capable injection through it) could copy a victim row's crypto columns into a row keyed by its own identity and have the daemon decrypt the victim's plaintext — the exact database-compromise case envelope-at-rest is meant to survive. It is **not** reachable by an unprivileged or external actor; it is a missing defense-in-depth binding. Separately, plaintext key material is never zeroed (`crypto_store.zig` frees `dek_plain` via `defer` without wiping it), so a core dump or swap image can retain it; and `crypto_store.zig` has zero tests over its lifecycle or its `NotFound` / `UnsupportedKekVersion` / `InvalidEnvelope` branches, with no guard that two encryptions draw distinct nonces.

**Solution summary:** thread an associated-data parameter through `encrypt`/`decrypt` and bind a canonical `(workspace_id, key_name, kek_version)` identity into both envelope legs; stamp new writes as `kek_version = 2` (AEAD-bound). Because binding associated data changes the envelope, existing rows written with empty associated data (`kek_version = 1`) will not decrypt under the bound path — so `load()` dispatches on the existing `kek_version` seam: version 1 decrypts with empty associated data (read-compatible), version 2 with the bound identity; a version-1 row is opportunistically upgraded to version 2 the next time it is stored/rotated (self-healing; no standalone migration). Add `secureZero` on every key buffer along success and error paths, and add the missing envelope lifecycle / error-branch / nonce-uniqueness tests.

## PR Intent & comprehension handshake

- **PR title (eventual):** Bind vault envelopes to row identity, zero key material, and cover the envelope lifecycle
- **Intent (one sentence):** a stolen or write-tampered `vault.secrets` row can no longer be relocated to decrypt another workspace's secret, transient key bytes do not linger in memory, and the envelope path is guarded by tests.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/secrets/crypto_primitives.zig` — `encrypt`/`decrypt` (`:77`, `:100`) pass `""` as the AEAD associated data; `EncryptedBlob`, `setKekFromHex` (`:47`, decoded `key` left on the stack), and the 4 existing tests are the surfaces §1/§2/§3 extend.
2. `src/agentsfleetd/secrets/crypto_store.zig` — `store()` wraps a per-row DEK under the process KEK then encrypts the payload under the DEK; `load()` already switches on `kek_version` and returns `UnsupportedKekVersion` for non-1 (`:104`) — that switch IS the version seam §1 builds on. Note the `dek_plain` `defer alloc.free` at `:126` frees without zeroing.
3. `src/agentsfleetd/state/tenant_provider.zig:70` + `src/agentsfleetd/http/handlers/fleets/secrets.zig:288` — the established `std.crypto.secureZero(u8, …)`-in-`deinit` convention §2 mirrors; the innermost store is the one place currently omitting it.
4. `dispatch/write_zig.md` — Memory Safety, Multi-Step Init errdefer, Type Design (tagged-union results), and the SQL Statement Modules rule (`§SQL Statement Modules`) §1 conforms to when the `INSERT` text changes.
5. `docs/AUTH.md` §Sensitive-data classification — the LLM-provider-key row and the "secrets belong in vault, resolved via `crypto_store.load()`" constant this diff must preserve, never weaken.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/secrets/crypto_primitives.zig` | EDIT | thread `ad: []const u8` through `encrypt`/`decrypt`; `secureZero` the decoded key in `setKekFromHex`; add nonce-uniqueness + associated-data-mismatch + empty-associated-data round-trip tests; document the KEK-wrap birthday bound as a named constant |
| `src/agentsfleetd/secrets/crypto_store.zig` | EDIT | build the canonical identity, bind it into both legs, stamp `kek_version = 2`, dispatch v1/v2 on read, `secureZero` `dek` / `kek` / `dek_plain` on every path; call the extracted SQL constants |
| `src/agentsfleetd/secrets/sql.zig` | CREATE | domain-local SQL statement + column-name constants for the store `INSERT` and load `SELECT` (the `kek_version` column edit changes the statement text — SQL Statement Modules rule) |
| `src/agentsfleetd/secrets/crypto_store_test.zig` | CREATE | envelope lifecycle, three error branches, row-relocation negative, v1 read-compat + rewrap, no-leak-on-error-path, source-grep zeroization assertion |
| `src/agentsfleetd/tests.zig` | EDIT | register `secrets/crypto_store_test.zig` in the aggregate test root |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **CTM** (the AEAD tag check is constant-time by construction; no new short-circuit secret compare is introduced — hold the line), **VLT** (secrets stay in `vault.secrets`, resolved via `crypto_store.load()`; this diff strengthens the boundary, must not add a plaintext column), **TGU** (`SecretError` stays a flat error set; no optional-field variant structs added), **UFS** (`kek_version` values, the associated-data field separator, and the birthday-bound magnitude become named constants — no bare `1`/`2`/`32`), **NSQ** (schema-qualified, named-constant SQL in the new `sql.zig`), **TST-NAM** (new test identifiers milestone-free), **ORP** (no symbol renamed/deleted — none to sweep, table below records N/A).
- **`dispatch/write_zig.md`** — memory-safety (return-slice ownership, `std.testing.allocator` leak proofs), multi-step `errdefer` on `store()`'s two allocations, DRAIN (`load()`'s `PgQuery` already drains — preserve), SQL Statement Modules (extract inline SQL on touch), cross-compile both linux targets.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — two `*.zig` production files + one new source + one new test | cross-compile `x86_64-linux` + `aarch64-linux`; `make memleak` for the allocator-touching `store`/`load` |
| PUB / Struct-Shape | yes — new `sql.zig`; changed `encrypt`/`decrypt` shape | `sql.zig` is a stateless constants/function namespace → conventional layout (one-sentence "operations-over-value, no owned state"); `buildAad` stays private; `encrypt`/`decrypt` keep their free-function shape, only the parameter list grows |
| File & Function Length (≤350/≤50/≤70) | yes — watch `crypto_primitives.zig` (171) as inline tests grow | SQL extraction shrinks `crypto_store.zig`; if `crypto_primitives.zig` crosses ~300 with the new tests, extract them to a sibling `crypto_primitives_test.zig` registered in `tests.zig` |
| UFS | yes | `KEK_VERSION_LEGACY = 1`, `KEK_VERSION_AAD_BOUND = 2`, the associated-data field separator, and `KEK_WRAP_NONCE_BIRTHDAY_BOUND_LOG2 = 32` as named constants |
| UI Substitution / DESIGN TOKEN | no | no UI surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LIFECYCLE yes; others no | LIFECYCLE: `secureZero`+`free` pairing and `errdefer` placement reviewed per the façade. No new log event (existing `stored`/`retrieved` info logs unchanged). No new `UZ-*` code — the three error branches already exist. No `schema/*.sql` DDL change: `kek_version` already exists with a Data Definition Language (DDL) `DEFAULT 1`; writing `2` is app-level |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/state/tenant_provider.zig:70` — the `std.crypto.secureZero(u8, self.api_key)`-in-`deinit` convention §2 replicates for the transient key buffers. Alignment: exact call shape; divergence: crypto_store's buffers are function-local stack/heap, so the zero is a `defer` at the allocation site, not a struct `deinit`.
- **Reference:** `src/agentsfleetd/fleet_bundle/sql.zig` — the domain-local `sql.zig` shape (named `SELECT_*`/`INSERT_*` constants imported by the parent module) §1's extraction mirrors. Divergence: none.

## Sections (implementation slices)

### §1 — Bind row identity into the envelope; version the format

`encrypt`/`decrypt` gain an `ad: []const u8` parameter passed straight to the AES-256 Galois/Counter Mode (GCM) call. `store()` builds a canonical identity — `workspace_id`, a field separator, `key_name`, the separator, and `kek_version` — and passes it as the associated data for **both** the DEK-wrap and the payload encryption, stamping the row `kek_version = 2`. Because associated data is part of the authenticated envelope, a version-1 row (written with empty associated data) cannot decrypt under the bound path; `load()` therefore dispatches on the existing `kek_version` seam. **Implementation default:** version-1 rows decrypt with empty associated data (read-compatible) and are rewritten as version-2 on their next `store()`/rotation — self-healing, no standalone migration — because the residual version-1 exposure is the same already-privileged-write-foothold case the verifiers bounded, and a lazy upgrade avoids a bulk backfill for a defense-in-depth gap. `load()` continues to reject any `kek_version` outside `{1, 2}` with `UnsupportedKekVersion`.

- **Dimension 1.1** — `encrypt` then `decrypt` with a matching associated data round-trips; `decrypt` with a *different* associated data returns `DecryptFailed` (relocation proof at the primitive level) → Test `test_aead_ad_mismatch_fails`
- **Dimension 1.2** — `store()` writes `kek_version = 2` and both legs authenticate only against the row identity; `load()` round-trips the value → Test `test_store_load_v2_roundtrip`
- **Dimension 1.3** — a version-2 row whose crypto columns are copied into a different `key_name` fails `DecryptFailed` on `load()` → Test `test_store_relocated_row_rejected`
- **Dimension 1.4** — a legacy version-1 row (empty associated data) still decrypts on `load()`, and a subsequent `store()` rewrites it as version 2 → Test `test_store_v1_read_compat_then_rewrap`

### §2 — Zero key material on every path

Every plaintext key buffer is wiped before its backing storage is released, on success and error paths. **Implementation default:** a `defer std.crypto.secureZero(u8, &buf)` beside each stack buffer (`dek`, the `kek` copies in `store`/`load`, the fixed `dek` in `load`, the decoded `key` in `setKekFromHex`), and a zero-then-free on the heap `dek_plain` in `load()`. **Honest bound:** the KEK also lives for the process lifetime in the `g_kek` global, so a core dump always contains the KEK regardless of these transient-copy wipes — the DEK and `dek_plain` zeroing is the load-bearing part; the KEK-copy wipes are defense-in-depth against a stray stack/swap image, not a claim that the vault's master key is otherwise protected from a memory capture.

- **Dimension 2.1** — every key-material buffer named above has an adjacent `secureZero`; enforced by a source assertion so review discipline cannot regress it → Test `test_key_zeroization_present`
- **Dimension 2.2** — `load()`'s decrypt-failure branch still frees `dek_plain` (zero leak under `std.testing.allocator`), proving the cleanup runs on the error path → Test `test_load_error_path_frees_dek`

### §3 — Cover the envelope lifecycle, its error branches, and nonce uniqueness

`crypto_store.zig` has no tests today. Add the lifecycle and each `load()` failure branch, plus a nonce-uniqueness guard on `encrypt`.

- **Dimension 3.1** — two `encrypt` calls on identical plaintext + key + associated data yield distinct nonces → Test `test_encrypt_nonce_unique`
- **Dimension 3.2** — `load()` for an absent `(workspace_id, key_name)` returns `NotFound` → Test `test_load_not_found`
- **Dimension 3.3** — `load()` on a row seeded with `kek_version = 3` returns `UnsupportedKekVersion` → Test `test_load_unsupported_kek_version`
- **Dimension 3.4** — `load()` on a row with a wrong-length `dek_nonce` column returns `InvalidEnvelope` → Test `test_load_invalid_envelope`
- **Dimension 3.5** — a second `store()` for the same key upserts, and `load()` returns the latest value → Test `test_store_upsert_roundtrip`

### §4 — Confirm payload nonces safe; document the KEK-wrap bound

Payload encryption uses a fresh per-secret DEK, so a payload-nonce collision across distinct DEKs is harmless — no fix needed there. The only single-key random-nonce path is the KEK-wrap, whose NIST SP 800-38D birthday bound (~2^32 wraps) is recorded as an accepted, documented limit; the associated-data binding from §1 further localizes any collision to one identity. **Implementation default:** document the bound as a named constant with rationale rather than switch to a nonce-misuse-resistant construction — that larger crypto change is Out of Scope and the bound is far from reachable write volume.

- **Dimension 4.1** — two `store()` calls for the same plaintext produce distinct wrapped-DEK and distinct payload ciphertext (fresh DEK per secret) → Test `test_store_fresh_dek_per_secret`
- **Dimension 4.2** — the accepted KEK-wrap birthday bound is a named constant with a rationale doc-comment (no silent magic number) → Test `test_kek_wrap_bound_documented`

## Interfaces

```
crypto_primitives.encrypt(alloc, plaintext, ad: []const u8, key) !EncryptedBlob
crypto_primitives.decrypt(alloc, nonce, ciphertext, tag, ad: []const u8, key) ![]u8
   — associated data threaded to both AES-256-GCM calls (was the empty string).

crypto_store.store(alloc, conn, workspace_id, key_name, plaintext) !void  — UNCHANGED signature;
   now binds (workspace_id, key_name, kek_version) into both legs and writes kek_version = 2.
crypto_store.load(alloc, conn, workspace_id, key_name) ![]u8  — UNCHANGED signature;
   dispatches decrypt on kek_version (1 = empty AD, 2 = bound AD); rejects other versions.

No HTTP route, request/response shape, or public vault surface changes. vault.zig callers
(store/load) are untouched — the identity is derived inside store/load from arguments already held.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Relocated crypto columns | write-foothold actor copies a victim row's blobs into its own `key_name` | associated-data authentication fails; `load()` returns `DecryptFailed`; no plaintext disclosed (Test 1.3) |
| Legacy version-1 row read | row predates this change (empty associated data) | `load()` decrypts it with empty associated data; next `store()` upgrades it to version 2 (Test 1.4) |
| Missing secret | no `(workspace_id, key_name)` row | `load()` returns `NotFound`, logged at debug (Test 3.2) |
| Unsupported KEK version | `kek_version` outside `{1, 2}` | `load()` returns `UnsupportedKekVersion`, logged at error (Test 3.3) |
| Malformed envelope column | wrong-length `dek_nonce`/`dek_tag`/`nonce`/`tag` BYTEA | `toFixed` returns `InvalidEnvelope` (Test 3.4) |
| Decrypt failure mid-load | tampered payload or wrong key | error path frees `dek_plain` (no leak) and returns the error (Test 2.2) |

## Invariants

1. Every version-2 envelope authenticates only against its own `(workspace_id, key_name, kek_version)` on both legs — enforced by the relocation negative Test 1.3.
2. New writes emit `kek_version = KEK_VERSION_AAD_BOUND`; `load()` decrypts version 1 with empty associated data and rejects any version ∉ `{1, 2}` — enforced by the named constants + Test 3.3.
3. Every key-material buffer (DEK, KEK stack copies, unwrapped DEK, decoded master key) is `secureZero`'d before its storage is released, on success and error paths — enforced by the source-assertion Test 2.1.
4. `store()` draws a fresh per-secret DEK, from which payload-nonce uniqueness follows — enforced by Test 4.1.
5. `store()`/`load()` leak zero bytes on their error paths — enforced by Test 2.2 under `std.testing.allocator`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | existing `stored` / `retrieved` info logs fire unchanged (key_name only, never the secret) | unchanged | unchanged — no key material or plaintext in any log | existing `store`/`load` info-log behavior asserted indirectly by the lifecycle tests |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_aead_ad_mismatch_fails` | encrypt with AD=identityA; decrypt with AD=identityB → `DecryptFailed`; decrypt with AD=identityA → plaintext |
| 1.2 | integration | `test_store_load_v2_roundtrip` | store then load a secret → equal plaintext; the stored row carries `kek_version = 2` |
| 1.3 | integration (negative) | `test_store_relocated_row_rejected` | copy row A's crypto columns into row B (different key_name) → `load(B)` returns `DecryptFailed` |
| 1.4 | integration (regression) | `test_store_v1_read_compat_then_rewrap` | seed an empty-AD `kek_version = 1` row → `load` decrypts it; after `store`, the row is `kek_version = 2` |
| 2.1 | unit (source-grep) | `test_key_zeroization_present` | `@embedFile` of both crypto sources contains a `secureZero` for `dek`, `kek`, `dek_plain`, and the decoded key |
| 2.2 | unit (leak) | `test_load_error_path_frees_dek` | injected decrypt failure in `load` under `std.testing.allocator` → zero leaks reported |
| 3.1 | unit | `test_encrypt_nonce_unique` | two `encrypt(plaintext, key, ad)` calls → `blob1.nonce != blob2.nonce` |
| 3.2 | integration (negative) | `test_load_not_found` | `load` for an unseeded key → `NotFound` |
| 3.3 | integration (negative) | `test_load_unsupported_kek_version` | seed `kek_version = 3` → `load` returns `UnsupportedKekVersion` |
| 3.4 | integration (negative) | `test_load_invalid_envelope` | seed a 4-byte `dek_nonce` → `load` returns `InvalidEnvelope` |
| 3.5 | integration | `test_store_upsert_roundtrip` | store v1-value then store v2-value for one key → `load` returns v2-value |
| 4.1 | integration | `test_store_fresh_dek_per_secret` | store the same plaintext twice → distinct `encrypted_dek` and distinct `ciphertext` columns |
| 4.2 | unit (source-grep) | `test_kek_wrap_bound_documented` | `KEK_WRAP_NONCE_BIRTHDAY_BOUND_LOG2` constant present with a rationale doc-comment |

Regression: 1.4 and 3.5 protect existing read/upsert behavior. Idempotency: `store`'s `ON CONFLICT` upsert is covered by 3.5. Integration rows seed rows through the real `vault.secrets` schema (no temp tables) and skip gracefully when `TEST_DATABASE_URL` is unset, mirroring `vault_test.zig`.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Relocated row + version binding (§1) proven | `make test-integration` | exit 0 incl. `test_store_relocated_row_rejected`, `test_store_v1_read_compat_then_rewrap` | P0 | |
| R2 | Envelope error branches + nonce guard (§3) proven | `make test` | exit 0 incl. the four §3 negative/lifecycle tests + `test_encrypt_nonce_unique` | P0 | |
| R3 | Key material zeroed on every path (§2) | `grep -c "secureZero" src/agentsfleetd/secrets/crypto_store.zig src/agentsfleetd/secrets/crypto_primitives.zig` | each file ≥ 1; total ≥ 4 | P0 | |
| R4 | No plaintext-secret column added (VLT held) | `grep -in "plaintext\|secret_value" schema/002_vault_schema.sql` | no output | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes (vault DB path touched) | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted, no public symbols renamed or removed. `encrypt`/`decrypt` gain a parameter (all four call sites live in `crypto_store.zig` and are updated in the same diff); no orphaned import or reference is created.

## Out of Scope

- A bulk background rewrap job that upgrades every version-1 row to version 2 eagerly — the lazy on-write upgrade is sufficient for a defense-in-depth gap; a backfill is future work.
- Switching the KEK-wrap to a nonce-misuse-resistant construction (AES-GCM-SIV) or a deterministic wrap nonce — the documented birthday bound is accepted at realistic write volume (§4).
- Protecting the process-lifetime `g_kek` global from a memory capture (it necessarily lives resident for the daemon to decrypt) — the honest bound stated in §2.
- Any change to the `vault.secrets` schema shape, the `api_runtime` grant, or the public vault / HTTP surface.

---

## Product Clarity (authoring record)

1. **Successful user moment** — internal hardening: the observable moment is a test suite where relocating a victim's encrypted columns into an attacker-keyed row yields `DecryptFailed` instead of the victim's plaintext, and where a killed daemon's memory image no longer carries a live DEK.
2. **Preserved user behaviour** — every existing store/rotate/resolve path keeps its exact semantics and public signatures; a secret written before this change still resolves (version-1 read-compat), and the happy-path store/load round-trip is byte-for-byte unchanged.
3. **Optimal-way check** — binding identity via AEAD associated data is the established, minimal fix (AWS Encryption SDK, Tink use encryption context the same way); reusing the existing `kek_version` seam avoids a new column or a migration.
4. **Rebuild-vs-iterate** — iterate: three contained edits on one crypto module, each mirroring an in-repo convention; no redesign, and nothing here trades away run-to-run determinism.
5. **What we build** — one associated-data parameter bound into both legs, a version dispatch on read, `secureZero` on every key buffer, one extracted SQL module, and the missing envelope tests.
6. **What we do NOT build** — a bulk rewrap job, a nonce-misuse-resistant KEK-wrap, or any protection of the resident master key — all named in Out of Scope.
7. **Fit with existing features** — compounds the vault credential boundary (`docs/AUTH.md`); must not destabilize `vault.zig`'s structured-credential layer or the runner-lease provider-key delivery that consumes resolved secrets.
8. **Surface order** — N/A — no user surface; this is backend crypto hardening with no Command-Line Interface (CLI) or UI change.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; the only "user" is an on-call operator, whose signal is the existing `unsupported_kek_version` / `decrypt_failed` error logs, unchanged by this spec.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections mapping 1:1 to the four verified findings — identity binding, key zeroing, lifecycle tests, and the documented nonce bound — each independently testable and DONE-markable on one crypto module.
- **Alternatives considered:** (a) an explicit migration that eagerly rewraps every version-1 row — rejected for now: it needs a backfill path and write amplification for a gap already gated behind an existing write foothold, so the lazy on-write upgrade is the proportionate move; (b) a new `envelope_version` column distinct from `kek_version` — rejected: the existing `kek_version` switch is the seam the guidance points at, and a second version column would fragment the dispatch.
- **Patch-vs-refactor verdict:** this is a **patch** — it closes a specific binding gap, a specific zeroing gap, and a test gap on an otherwise-correct module, mirroring in-repo conventions; the only structural touch (SQL extraction) is forced by the SQL Statement Modules rule and *shrinks* the module.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: empty at creation.
- **Metrics review** — empty at creation.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
