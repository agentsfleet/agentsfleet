# M102_003: Models & Keys — credential metadata seam + Active-Model hero

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 003
**Date:** Jun 28, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing model/key surface + the credential read API both change
**Categories:** API, CLI, UI
**Batch:** B1 — §1–§2 (backend seam) land before §3–§6 consume the shape; §7 (docs) gates CHORE-close
**Branch:** feat/m102-agent-identity-proxy — rides PR #458 (per Indy, 2026-06-28)
**Test Baseline:** unit=2204 integration=208 (zig test-depth gate at CHORE-open)
**Depends on:** M102_001 (shares the credential vault + tenant_provider surfaces #458 already opens)
**Provenance:** agent-generated (pre-spec) — live chat scoping with Indy 2026-06-28; supersedes the spec-less redesign that previously rode #458 (resolves prior Risk R1).

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §8 (credential body + api_key visibility boundary), §10 (cap.json). This spec adds a non-secret metadata *projection* without changing the M45 opaque-encrypted-body invariant.

---

## Implementing agent — read these first

1. `docs/architecture/billing_and_provider_keys.md` §8 — api_key visibility boundary (process-internal vs user-facing); body `{provider, api_key, model, base_url?}`; **user-chosen names** (classify by `provider`, never the name).
2. `src/agentsfleetd/http/handlers/fleets/credentials.zig` — list/store/delete to extend; `fetchCredentialListOnConn` is the projection site; `workspace_guards.enforce(… .operator)` is the gate.
3. `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` — the existing decrypt-body-then-zero pattern the list projection mirrors.
4. `docs/AUTH.md` + `docs/REST_API_DESIGN_GUIDELINES.md` — auth boundary + REST shape for the PATCH rotate + the list field addition.
5. `docs/design/models-creds-variant-C2-hero-flow.html` (C2 page) + `ui/packages/app/lib/api/client.ts` (same-origin `/backend` proxy → CORS-free client cap.json fetch).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** rides #458; this workstream's commits read `feat(m102): models & keys — credential metadata seam + active-model hero`
- **Intent:** one combined Models & Keys page where the live model is a hero and every stored provider key is one click away — backed by the server telling the client what each credential *is*, so classification, model-memory, and key-rotation stop being browser guesses.
- **Handshake (filled):** project non-secret descriptors the vault already stores (decrypt the body on list, extract all-but-`api_key`), add a key-only rotate, let the client read `kind` instead of guessing; build C2; delete the option-card flow; move cap.json to a once-per-session client fetch out of the RSC payload. `ASSUMPTIONS:` (1) **decrypt-on-list** keeps the M45 opaque-body invariant (plaintext zeroed, api_key never returned) — over a metadata column (which would amend the doc + add a migration); (2) **switch-list is credential-driven** (one row per stored provider key, labelled from metadata) — supersedes the fixed 3-row mock, matching the doc's multi-credential support; (3) one-click **Switch sets the credential's stored model**; (4) **Replace-key uses PATCH** (safe for every kind); (5) e2e is not in PR CI — run locally / note in the PR.

---

## Product Clarity

1. **Successful user moment** — operator opens Models & Keys, sees "`claude-sonnet-4-6` · via `anthropic-prod` · LIVE", clicks Switch on their OpenAI row, and the hero flips in one click — no key re-entry.
2. **Preserved behaviour** — platform default stays keyless; custom secrets still add/list/rotate; `tenant provider add/delete` CLI unchanged; the install-preview credential deep-link still resolves; api_key never in any response.
3. **Optimal-way check** — the optimal shape is "the server says what each credential is"; the accepted gap is decrypt-on-list (small crypto on a cold page) vs a metadata column (cleaner but amends the vault invariant + migration). The settings list is not a hot path.
4. **Rebuild-vs-iterate** — refactor, not patch: a name-allowlist leaves classification a guess and can't fix custom-endpoint rotation or model-memory. Determinism *improves* (classification becomes a server fact).
5. **What we build** — list metadata projection; PATCH rotate; tagged-union client Credential; client cap.json provider; C2 page; two consolidated forms; read-only `integration` CLI command.
6. **What we do NOT build** — a metadata sidecar column (follow-up); per-provider model *history* beyond the stored model; writable `integration` CLI; Stripe/billing (v2.1).
7. **Fit** — compounds with M102_001's credential/integration work in #458; must not destabilise the lease-path `tenant_provider_resolver` reading the same credentials.
8. **Surface order** — both; API seam is the foundation, UI the headline, CLI a read-only mirror. Backend-first within the workstream.
9. **Dashboard restraint** — no per-provider "last used" / usage charts (no counter behind them); the switch-list shows only what metadata proves (provider + model), never a fabricated reference.
10. **Confused-user next step** — an unconfigured provider self-serves via inline "Add key & model"; a failed switch shows the typed error, model unchanged; `agentsfleet integration list` mirrors connect-state.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (provider ids / `kind` values / field names as named consts, cross-runtime Zig↔TS parity), NDC/NLR/ORP (dead-code sweep), TGU (tagged-union Credential), TST-NAM, FLL.
- **`dispatch/write_zig.md`** — §1–§2: pg-drain lifecycle, tagged-union results, multi-step `errdefer` on decrypt buffers, cross-compile both linux targets.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — PATCH rotate route + list field addition (URL design, route registration, handler signature, error shape).
- **`docs/AUTH.md`** — api_key visibility boundary; the list decrypt path zeroes plaintext + never returns/logs api_key.
- **`dispatch/write_ts_adhere_bun.md`** — §3–§6 client (const discipline, tagged unions, UI substitution, design tokens).
- **`docs/LOGGING_STANDARD.md`** + ERROR REGISTRY — rotate error codes, same commit. No `SCHEMA_CONVENTIONS.md` (Option A adds no column).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — §1–§2 | cross-compile both targets; `conn.query().drain()`; tagged-union results; `errdefer` zero on decrypt buffers |
| PUB / Struct-Shape | yes | shape verdict for new list-row fields + rotate handler |
| File & Function Length (≤350/≤50/≤70) | yes | split C2 page into hero / switch-list / forms; extract projection helper in `credentials.zig` |
| UFS | yes | `kind` values, provider ids, field names as named consts; Zig `kind` ↔ TS identifier verbatim |
| UI Substitution / DESIGN TOKEN | yes — §4 | design-system primitives only; `theme.css` tokens; no arbitrary `*-[…]` where a token exists |
| LOGGING / ERROR REGISTRY | yes — §2 | `UZ-*` rotate codes registered same commit; api_key never in a log field |
| SCHEMA GUARD | no | Option A adds no column/migration |

---

## Overview

**Goal (testable):** `GET …/credentials` returns each credential's `kind ∈ {provider_key, custom_endpoint, custom_secret}` plus non-secret `provider`/`model`/`base_url` and **never** `api_key`; the Models & Keys page renders the active model as a hero and every stored provider key as a one-click Switch row classified by server `kind`; `make test-unit-app` stays at 100%.

**Problem:** the list returns only `{name, created_at}`, so the browser guesses what each credential is — a stored-but-inactive Anthropic key misfiles as a custom secret, custom-endpoint key rotation can corrupt the saved `base_url`, and "switch provider" can't remember the model. Names are user-chosen, so name-based guessing is structurally unfixable.

**Solution:** the server projects the non-secret descriptors it already stores (decrypting the body on the list path, extracting everything but `api_key`, zeroing the plaintext) and adds a key-only rotate. The client deletes its heuristics, models credentials as a tagged union keyed by `kind`, and renders the C2 hero + credential-driven switch list. cap.json moves to a once-per-session client fetch (same-origin `/backend` → no CORS) out of the RSC payload.

---

## Prior-Art / Reference Implementations

- **API** → `src/agentsfleetd/http/handlers/fleets/credentials.zig` (extend list; mirror `workspace_guards.enforce` + `PgQuery` drain) + `runner/credentials_mint.zig` (decrypt-then-`secureZero`); REST guide for the PATCH route.
- **UI** → design-system primitives + `theme.css` tokens; reuse `Step2Model`, `InlineProviderKeyCreate`, `CustomEndpointOwnKey`, `CustomSecretsList`, `AddCredentialFormDynamic`.
- **CLI** → the 7 Pillars (handler purity, output-as-a-service, structured JSON errors, auto-JSON when piped); mirror `tenant provider show` for read-only `integration list/show`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/fleets/credentials.zig` | EDIT | list projects `{kind,provider,model,base_url}` (decrypt+zero); add rotate handler |
| `src/agentsfleetd/http/{router,route_table,route_table_invoke}.zig` | EDIT | register PATCH `…/credentials/{name}` |
| `src/agentsfleetd/state/vault.zig` + `errors/error_registry.zig` | EDIT | decrypt-to-project + key-only update helpers; rotate `UZ-*` codes |
| backend `credentials*_test.zig` | CREATE/EDIT | unit + integration incl. negatives |
| `ui/packages/app/lib/api/credentials.ts` + `model_caps.ts` + new `ModelCatalogueProvider` | EDIT/CREATE | tagged-union `Credential`, `rotateCredential`, client-fetchable cap.json + provider-list helper |
| `…/settings/models/page.tsx` + `components/{ActiveModelHero,ProviderSwitchList,ProviderKeyForm,CustomEndpointForm}.tsx` | EDIT/CREATE | C2 page; drop catalogue from RSC fetch; two consolidated forms (`activate` flag) |
| `…/credentials/page.tsx`, `components/layout/Shell.tsx`, `lib/fleet-credentials.ts` (+ test) | EDIT | redirect `/credentials`→`/settings/models`; nav "Models & Keys" (remove Credentials); repoint `WORKSPACE_CREDENTIALS_PATH` + rewrite its comment in lockstep |
| `ProviderSelector`, `Step1Credential`, `ProviderCredentialRows`, `CustomEndpointForm`-component, `OwnKeyKind` toggle (+ tests, `dashboard-app-mocks` stub) | DELETE | superseded — Dead Code Sweep |
| `cli/src/**/integration*.ts` + `cli/test/acceptance/integration*.spec.ts` | CREATE | read-only `integration list/show` + acceptance |
| `tests/e2e/acceptance/{settings-models,provider-credential-reference,credentials-lifecycle,integrations-nav}.spec.ts` | EDIT/CREATE | rewrite drifted + add nav spec |
| `docs/architecture/*.md` (all 10 — §7) + `~/Projects/docs/**/*.mdx` + `changelog.mdx` | EDIT | reconcile + changelog; two per-file argue-back reports |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen:** backend seam (§1–§2) first so the client (§3–§5) reads a real `kind`; e2e (§6) and docs (§7) last. Metadata via **decrypt-on-list (Option A)**.
- **Alternatives:** **Option B — metadata column**: list never decrypts (safer steady-state) but amends the M45 opaque-body invariant (doc §8.1) + migration + backfill — rejected for scope/coupling; named follow-up. **Name-allowlist patch**: rejected — can't fix rotation or model-memory; classification stays a guess.
- **Verdict:** refactor, decrypt-on-list. Option B is the named follow-up, not a silent mud-patch.

---

## Sections (implementation slices)

### §1 — Credential metadata projection on list — ✅ implemented (unit local; integration in CI)

Two passes over one `pg.Conn` (a second open read result on the same connection is forbidden): pass 1 materializes the stored keys then drains the query; pass 2 `vault.loadJson`s each row (an N+1 that is fine on the cold settings page) and projects via the pure `credential_metadata`. `kind` is derived from the `provider` field's presence/value — `openai-compatible`→`custom_endpoint`; any other string `provider`→`provider_key`; missing/non-string→`custom_secret` (accepted edge: a custom secret carrying a `provider` string misfiles as a provider key). Returns non-secret `provider`/`model`/`base_url`; **api_key is never read into the response** (the projection type has no field for it); unparseable/legacy bodies degrade to `custom_secret` (list still 200). Split for FLL: pure classify/project in `credential_metadata.zig` (unit-tested), DB orchestration in `credential_list.zig`.

- **1.1** `kind` from `provider`, not name → `test_list_classifies_by_provider`
- **1.2** response carries provider/model/base_url, never api_key → `test_list_omits_api_key`
- **1.3** unparseable body → `custom_secret`, still 200 → `test_list_degrades_unparseable_body`
- **1.4** operator gate + pg-drain unchanged → `test_list_requires_operator`

### §2 — Key-only credential rotate — ✅ implemented (unit local; integration in CI)

`PATCH …/credentials/{name}` `{api_key}` updates only the secret, preserving provider/model/base_url — Replace-key safe for every kind. The handler loads the stored object, `put`s only `api_key` on the parse-arena map, re-stringifies (4 KB cap → `UZ-VAULT-002`), re-stores, then `secureZero`s its duped key copy. Missing row → `UZ-VAULT-003` 404; empty key → `UZ-REQ-001` 400. The item route is method-neutral (`workspace_credential`: PATCH + DELETE).

- **2.1** rotate preserves non-secret fields → `test_rotate_preserves_nonsecret_fields`
- **2.2** missing credential → typed 404 → `test_rotate_missing_404`
- **2.3** empty/oversized key rejected; key never logged → `test_rotate_validates_key`

### §3 — Client domain model + reads — ✅ implemented (100% test-unit-app)

Tagged-union `Credential` keyed by server `kind`; provider list from credentials' metadata ∪ `uniqueProviders(model_caps)` + a small label map; `getTenantProvider`/`listCredentials` `cache()`-wrapped; cap.json fetched once client-side via `ModelCatalogueProvider`, out of the RSC payload.

- **3.1** classification reads `kind`, no name heuristic → `test_credential_union_from_kind`
- **3.2** provider rows from metadata + catalogue, labelled → `test_provider_rows_from_metadata`
- **3.3** catalogue fetched once, same-origin, cached; not in RSC fetch → `test_catalogue_fetches_once`
- **3.4** provider/credential reads deduped per render → `test_reads_deduped`

### §4 — C2 page + form consolidation — ✅ implemented (100% test-unit-app)

Active-Model hero (LIVE/DEFAULT pill, model, credential-ref chip, Provider/Context/Billing meta-grid; Change model / Replace key / Switch-to-platform) + credential-driven switch list + separate custom-secrets section. Six forms collapse to `ProviderKeyForm` + `CustomEndpointForm` (each with an `activate` flag).

- **4.1** hero renders active model + actions → `test_hero_renders_active`
- **4.2** Change model → `setProviderSelfManaged(ref, model)` (provider-scoped picker) → `test_change_model`
- **4.3** Replace key → PATCH rotate (named) / re-entry (custom) → `test_replace_key`
- **4.4** keyed → one-click Switch (stored model); unkeyed → inline Add+activate; platform → reset → `test_switch_one_click`, `test_add_key_and_activate`
- **4.5** provider keys never appear under custom secrets → `test_custom_secrets_excludes_provider_keys`

### §5 — Nav, routing, Dead Code Sweep — ✅ implemented (100% test-unit-app)

Nav "Models & Keys" (Credentials item removed); `/credentials`→`/settings/models` redirect; `WORKSPACE_CREDENTIALS_PATH` repointed with its comment + routing test in lockstep; option-card flow + tests deleted.

- **5.1** nav shows "Models & Keys", no Credentials item → `test_nav_models_keys`
- **5.2** `/credentials` redirects → `test_credentials_redirect`
- **5.3** repointed path + comment + test consistent → `test_workspace_credentials_path`
- **5.4** deleted files gone, zero orphan refs → grep sweep

### §6 — e2e (UI + CLI)

- **6.1** rewrite `settings-models` / `provider-credential-reference` / `credentials-lifecycle` to the combined page
- **6.2** `integrations-nav.spec.ts`: nav→`/integrations` + Models & Keys renders
- **6.3** `agentsfleet integration list` / `integration show <id>` read-only → unit + acceptance (mirror `tenant-provider-mutation`)
- **6.4** CLI `tenant provider` + `credential` round-trip acceptance

### §7 — Docs reconciliation + changelog (DOCUMENT stage; gates CHORE-close) — ✅ done

Re-read the shipped diff, then walk **every** `docs/architecture/*.md` and **every** product/capability `*.mdx` in `~/Projects/docs/` (own-branch `chore/m102-models-keys-credential-seam` off `main`). Update each surface the list shape / rotate / Models & Keys UX changes; add a `changelog.mdx` Update. Produce **two per-file argue-back reports** (file → updated+what / not-updated+why) in PR Session Notes + Discovery. Silent skip = CHORE-close violation.

- **7.1** ✅ all 13 `docs/architecture/*.md` reviewed; UPDATE: `billing_and_provider_keys.md` (§8.3 list+rotate, §8.2 boundary), `product_analytics.md` (events + group/person context), `user_flow.md` (§8.7 dashboard equivalent); NO-CHANGE for the other 10 with argue-backs → Discovery Report A
- **7.2** ✅ docs-repo `*.mdx` reviewed (PR #112); UPDATE: `fleets/credentials.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx` (UZ-VAULT-003, recon-miss caught); NO-CHANGE swept with argue-backs → Discovery Report B + Session Notes
- **7.3** ✅ `changelog.mdx` Jun 28 `<Update>` (supersedes Jun 24 "two destinations"; append-only) — docs PR #112

---

## Interfaces

```
GET /v1/workspaces/{ws}/credentials
200 { "credentials": [
  { "name":"anthropic-prod", "created_at":1777507200000,
    "kind":"provider_key", "provider":"anthropic", "model":"claude-sonnet-4-6" },
  { "name":"vllm-gw", "created_at":…, "kind":"custom_endpoint",
    "provider":"openai-compatible", "model":"…", "base_url":"https://…" },
  { "name":"STRIPE_API_KEY", "created_at":…, "kind":"custom_secret" }
] }                                  // api_key NEVER present in any row

PATCH /v1/workspaces/{ws}/credentials/{name}   body { "api_key":"…" }
200 { "name":"anthropic-prod" }      // only api_key changed; 404 if absent

type Credential =
  | { kind:"provider_key";    name; created_at; provider; model }
  | { kind:"custom_endpoint"; name; created_at; provider; model; base_url }
  | { kind:"custom_secret";   name; created_at }
```

---

## Failure Modes

| Mode | Cause | Handling (response + observable) |
|------|-------|----------------------------------|
| Undecryptable row | legacy/corrupt body | classify `custom_secret`; list still 200; `debug` log (no key) |
| Rotate missing name | wrong/deleted credential | typed 404; UI error, model unchanged |
| Empty/oversized key | bad input | 400 typed; key never logged |
| Switch to deleted credential | stale client list | server 4xx; UI refresh + typed error |
| cap.json fetch fails (client) | network/edge | pickers degrade to free-text model entry; hero unaffected (reads tenant_provider) |
| tenant_provider fetch errors | backend blip | hero degrades to platform-default view; no fabricated reference |

---

## Invariants

1. **api_key never in any response or log** — CI grep over responses/logs after a self-managed run (M48-style) + §1 negative test.
2. **Decrypted body freed; api_key never copied out** — the list decrypt routes through `vault.loadJson`, whose `parsed.deinit()` frees the arena-backed plaintext (the read path frees but does **not** `secureZero`, matching `credentials_mint.zig`). The api_key is structurally unprojectable: `credential_metadata.Projection` has no api_key field, so a leak is a compile error, not a review catch. (The PATCH rotate additionally `secureZero`s its duped key copy after the re-store.)
3. **Classification only from server `kind`** — no provider/name string-compare in client classification (grep sweep; heuristic helpers deleted).
4. **List stays operator-gated** — `workspace_guards.enforce(.operator)` unchanged; `test_list_requires_operator`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 / 1.3 | unit | `test_list_classifies_by_provider`, `test_list_degrades_unparseable_body` | anthropic→provider_key, openai-compatible→custom_endpoint, unknown/malformed→custom_secret (200) |
| 1.2 / 1.4 | integration | `test_list_omits_api_key`, `test_list_requires_operator` | no api_key bytes in response; viewer role → 403; drain clean |
| 2.1 | integration | `test_rotate_preserves_nonsecret_fields` | post-rotate read keeps provider/model/base_url |
| 2.2 / 2.3 | unit | `test_rotate_missing_404`, `test_rotate_validates_key` | unknown→404; empty/oversized→400; no key in logs |
| 3.1–3.4 | unit | `test_credential_union_from_kind`, `test_provider_rows_from_metadata`, `test_catalogue_fetches_once`, `test_reads_deduped` | union narrows on kind; rows from metadata+catalogue; one fetch; deduped reads |
| 4.1–4.5 | unit | `test_hero_renders_active`, `test_change_model`, `test_replace_key`, `test_switch_one_click`, `test_add_key_and_activate`, `test_custom_secrets_excludes_provider_keys` | each action calls the right server action; provider keys excluded from secrets |
| 5.1–5.3 | unit | `test_nav_models_keys`, `test_credentials_redirect`, `test_workspace_credentials_path` | nav/route/path-constant behaviour |
| 6.1–6.4 | e2e | acceptance (UI Playwright + CLI subprocess) | combined page renders; nav routes; `integration list/show` read-only; provider/credential round-trip |

**Regression:** custom-secret add/list/rotate + `tenant provider add/delete` CLI + install-preview deep-link unchanged. **Idempotency:** repeated rotate with the same key is a no-op-equivalent.

---

## Acceptance Criteria

- [ ] `kind` projected, api_key absent; PATCH rotate preserves non-secret fields — verify: `make test-integration` (credentials suite)
- [ ] Models & Keys page + switch list — verify: `cd ui/packages/app && bun run test:coverage` (100%)
- [ ] dead flow gone — verify: `grep -rn "ProviderSelector\|Step1Credential\|ProviderCredentialRows\|OwnKeyKind" ui/packages/app | grep -v '\.test\.'` (empty)
- [ ] `make lint` clean · `make test` passes · `make memleak` clean (decrypt buffer)
- [ ] Cross-compile: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` · `gitleaks detect` clean · no added file over 350 lines
- [ ] §7 docs: both per-file reports in Session Notes; `changelog.mdx` Update present — verify: reports + `git -C ~/Projects/docs diff --stat`

---

## Eval Commands (post-implementation)

```bash
# E1: backend — make test && make test-integration && make memleak 2>&1 | tail -3
# E2: cross-compile — zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux
# E3: UI 100% — cd ui/packages/app && bun run test:coverage 2>&1 | tail -5
# E4: lint+leaks — make lint && gitleaks detect 2>&1 | tail -3
# E5: sweeps (empty=pass) — api_key in list-response fixtures; orphan grep "ProviderSelector\|Step1Credential\|ProviderCredentialRows\|OwnKeyKind"; 350-line over origin/main
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `…/settings/models/components/{ProviderSelector,Step1Credential}.tsx` | `test ! -f` each |
| `…/credentials/components/{ProviderCredentialRows,CustomEndpointForm}.tsx` (helpers extracted) | `test ! -f` / helpers-only |
| `tests/provider-selector.test.ts`, `tests/custom-endpoint-form.test.ts` | `test ! -f` each |

**2. Orphaned references — zero remaining.**

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| `ProviderSelector` / `Step1Credential` / `ProviderCredentialRows` / `OWN_KEY_KIND` | `grep -rn "<sym>" ui/packages/app` | 0 (incl. `dashboard-app-mocks` stub) |

---

## Discovery (consult log)

- **Indy decision (2026-06-28):** rides PR #458 in the m102 worktree — no new milestone/branch; worktree clean post-commit. One spec (trimmed under cap).
- **Architecture decision (2026-06-28):** metadata via **decrypt-on-list (Option A)** over a metadata column — keeps the M45 opaque-body invariant; Option B is the named follow-up.
- **Indy directive (2026-06-28):** at CHORE-close review **every** `docs/architecture/*.md` + every docs-repo product/capability `*.mdx`; update or **argue-back per file** (two reports); `changelog.mdx` before close. Cross-repo docs edit authorized this session (own-branch flow).
- **§1/§2 implementation (2026-06-28):** list projection is a two-pass materialize→drain→`vault.loadJson` (N+1, cold-page-acceptable); pure classify/project extracted to `credential_metadata.zig` (unit-tested, 3 tests) with DB orchestration in `credential_list.zig` (FLL split — neither was in the original Files-Changed table, added for the 350-line cap). Classification keys on the `provider` field. Rotate route renamed `delete_workspace_credential`→`workspace_credential` (method-neutral; a `delete_`-named route serving PATCH was an inaccurate name). New error `UZ-VAULT-003` (credential-not-found, 404). Read path frees the decrypted body via `parsed.deinit()` (no `secureZero`, matching `credentials_mint.zig`); api_key unprojectable by type.
- **VERIFY constraint (2026-06-28):** backend unit tests + `make lint-zig` (fmt/zlint/pg-drain/FLL/test-depth) + cross-compile (x86_64+aarch64-linux) all pass locally; the DB-backed credential integration tests compile clean but run in **CI** — local Docker daemon is down (`make _ensure-test-infra` fails). The telemetry/webhook `zig build test` failures are pre-existing/environmental (reproduce in isolation; `test-unit-agentsfleetd` was green on CI at base).
- **§3–§5 client batch (2026-06-28):** tagged-union `Credential` (kind-keyed) + `rotateCredential` + cached server reads + `ModelCatalogueProvider` (client cap.json, once-per-session via `useEffect` background fetch — deliberately non-blocking, not `use()`/Suspense, so the hero never waits on the catalogue); C2 hero rebuilt as `ActiveModelHero` + extracted `HeroChangeModelPanel`/`HeroReplaceKeyPanel` + `ProviderSwitchList`; 6→2 forms (`ProviderKeyForm`/`CustomEndpointForm`, each `activate`); nav→"Models & Keys", `/credentials`→`redirect`. 100% `test-unit-app` (1128 tests).
- **R6 decision (2026-06-28):** `WORKSPACE_CREDENTIALS_PATH` kept at `/credentials` (now redirects to `/settings/models`, which hosts the custom-secrets vault) — install-preview deep-links stay decoupled from the destination; the R6 inversion was unneeded, comment rewritten.
- **Dead-code judgment (2026-06-28):** `InlineProviderKeyCreate` + `CustomEndpointOwnKey` + `Step2Model` were orphaned once `Step1Credential`/`ProviderSelector` were deleted; their reusable logic (paste-detect via `detect-provider.ts`, `isHttpsUrl`/`BASE_URL_NOT_HTTPS` → `lib/custom-endpoint.ts`) was folded into the consolidated forms and the originals deleted (NDC/ORP). `detect-provider.ts` stays, now consumed by `ProviderKeyForm`. `CredentialsList` (pre-existing orphan, not in scope) was retyped to keep building, flagged for separate cleanup.
- **Telemetry alignment (Indy directive, Supabase reference, 2026-06-28):** added granular product events `model_changed`/`key_rotated`/`provider_reset` (one helper each in `settings/models/lib/track.ts`) so rotate/change/reset are instrumented like Supabase's granular telemetry. Page-view telemetry was already global (`capture_pageview`/`pageleave`/`autocapture` in `posthog.ts`) — Supabase-equivalent, left untouched. The Supabase "UX touches" (FormHeader-style `SectionHeader`, catalogue skeleton + error-alert) were built then **reverted per Indy — zero visual change**; only componentization + invisible telemetry retained.
- **Telemetry alignment folded into this PR (Indy directive, 2026-06-28):** Indy reviewed Supabase Studio's telemetry (`packages/common/{telemetry,posthog-client,consent-state}`) and directed: *"i want all the changes of telemetry in this branch, not a separate branch. if you are m102_003 then the PR must go in here. You decide on the priority order... Leverage from supabase as much as we can."* — explicit authorization to bundle cross-cutting telemetry into this PR (overrides scope-isolation). **Correction on record:** an earlier claim that "we're stricter than Supabase on PII" was wrong — Supabase runs autocapture OFF + a real CMP consent gate (fail-closed, init-gated); our `autocapture:true` + config-flag (not consent) is the weaker posture. Shipped here (the Supabase-modeled subset that's additive + safe without the consent rework): **group analytics** (`posthog.group("workspace", …)` bound from `Shell`, so events/pageviews are workspace-sliceable — mirrors Supabase `$groups`), **richer person properties** (`setPersonProperties` workspace_count/plan), and a **curated `workspace_switched` event**, all behind the existing pending-queue deferral. **Deferred to a follow-up "telemetry hardening" milestone** (the heavy/risky parts): autocapture-off + curated-event migration (app only; website autocapture stays — it's public/low-PII and test-locked on), CMP consent gate, a shared `@agentsfleet/telemetry` package (mirrors Supabase `packages/common`), and website migration.
- **§7 docs reconciliation (2026-06-28) — Report A: `docs/architecture/*.md` (in-repo, rides #458).** Walked all 13. Two recon false-positives were verified-down to NO-CHANGE.
  - **UPDATE `billing_and_provider_keys.md`** — new §8.3 documents the credential metadata list (`kind` projection, decrypt-on-list, api_key structurally unprojectable, degrade-to-`custom_secret`, operator-gated) + key-only PATCH rotate (preserves provider/model/base_url; UZ-VAULT-003/UZ-REQ-001); §8.2 boundary list now names `GET …/credentials`. Option B (sidecar column) noted as deferred.
  - **UPDATE `product_analytics.md`** — catalog gains `workspace_switched`/`model_changed`/`key_rotated`/`provider_reset` (+ `integration_requested`, a pre-existing catalog gap), with the verbatim `EVENT_PROP_KEYS`; new "Workspace group + person context" section (PostHog `group("workspace")` + `setPersonProperties`, pending-queue deferral); fixed two stale descriptions (`credential_added`/`model_added` referenced the dead "credentials-page"/"models wizard").
  - **UPDATE `user_flow.md`** — §8.7 gains the dashboard-equivalent (Models & Keys hero + switch-list + Replace key) for the CLI posture flow; notes `/credentials` redirect.
  - **NO-CHANGE `observability.md`** — argue-back: it scopes itself to `agentsfleetd`'s server-side signal *paths* + the deliberately-bare runner; the shipped telemetry is entirely client-side and belongs in `product_analytics.md` (its declared home). Documenting it here would duplicate. (Recon said UPDATE — overruled.)
  - **NO-CHANGE `capabilities.md`** — argue-back: the vault storage shape `{provider, api_key, model}` and the §3 guarantees are unchanged; the list `kind` is a read-time projection, additive; the api_key boundary cross-ref to `billing §8.2` stays valid. (Spec §7.1 predicted "likely"; on inspection nothing is stale. Recon agreed NO-CHANGE.)
  - **NO-CHANGE (8): `data_flow.md`** (lease/execute/report pipeline untouched; new endpoints are cold-path reads), **`direction.md`** (secret-injection-timing principle unaffected), **`fleet_bundles.md`** (bundle credential-reference path unchanged), **`high_level.md`** (thesis/pillars unchanged), **`README.md`** (TOC stable, no new file), **`roadmap.md`** (neither shipped item is deferred-scope), **`runner_fleet.md`** (runner exec + inline secret delivery unchanged), **`scaling.md`** (two cold-path endpoints, negligible vs lease throughput).
- **§7 docs reconciliation (2026-06-28) — Report B: `~/Projects/docs/*.mdx` (docs repo, branch `chore/m102-models-keys-credential-seam`, PR #112).**
  - **UPDATE `changelog.mdx`** — new Jun 28 `<Update>` (Models & Keys one page; list `kind` projection; Replace-key PATCH). Supersedes the Jun 24 "two destinations" entry; per changelog voice, history is append-only — Jun 24 left intact, not rewritten.
  - **UPDATE `fleets/credentials.mdx`** — repointed the stale "Models → Custom — OpenAI-compatible" dashboard reference to the Models & Keys page; `--json` list now carries the classification fields; documented in-place Replace-key.
  - **UPDATE `cli/agentsfleet.mdx`** — `credential list --json` now carries `kind` + non-secret descriptors. Argue-back: the **human table is unchanged** (CLI renders name+created_at; the API fields are `--json` passthrough only — the CLI `CredentialSummary` type does not surface them). The recon's "new columns" claim was a false-positive, corrected to a one-line `--json` note.
  - **UPDATE `api-reference/error-codes.mdx`** — **full registry reconciliation** (Indy directive, 2026-06-28): extracted all 113 `e("UZ-…")` registrations from `src/**.zig` and diffed both ways against the public table. Findings: (a) every one of the 106 documented codes is real — zero phantom codes; (b) `UZ-REQ-001` already present; (c) **6 new user-facing codes ship on #458, only 1 was documented** — added `UZ-VAULT-003` (404, M102_003) **and the 5 M102_001 integration codes** the recon missed: `UZ-CONN-001` (503), `UZ-CONN-002` (400), `UZ-CRED-001` (404), `UZ-GH-001` (409), `UZ-GH-002` (502) under a new "Integration connect & token mint" section. Scope note: the 5 CONN/CRED/GH codes are M102_001-domain but ship on the same PR — folded into PR #112 for public-table accuracy at #458 deploy (touch-it-fix-it). Only `UZ-NONEXISTENT-999` / `UZ-TEST-001` sentinels remain (deliberately) undocumented. The recon marked this file NO-CHANGE — **overruled**.
  - **NO-CHANGE** (swept): `api-reference/introduction.mdx`, `quickstart.mdx`, `concepts.mdx`, `workspaces/*.mdx`, `fleets/{authoring,tools,troubleshooting,webhooks,overview,templates,install,running}.mdx`, `cli/{configuration,flags,install}.mdx`, `billing/*.mdx`, `memory.mdx` — none describe the credential list shape, the credential UI, or the rotate path.
  - **Telemetry omitted from changelog (argue-back)** — group analytics / person props / `workspace_switched` are zero user-visible change; the changelog is user-facing only. Documented in `product_analytics.md` (Report A) instead.
- Skill-chain outcomes + Indy-acked deferral quotes appended as work proceeds.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count + final coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean OR every finding dispositioned |
| After push to #458 | `/review-pr` + `kishore-babysit-prs` | comments addressed; greptile polled to two empty rounds |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit (backend) | `make test` | {paste} | |
| Integration | `make test-integration` | {paste} | |
| UI coverage | `bun run test:coverage` | {paste} | |
| Memleak | `make memleak` | {paste} | |
| Cross-compile | `zig build -Dtarget=…` | {paste} | |
| Lint + gitleaks | `make lint` / `gitleaks detect` | {paste} | |
| api_key + orphan sweep | E5 | {paste} | |

---

## Out of Scope

- **Option B** — non-secret metadata sidecar column + migration (read-path hardening follow-up).
- Per-provider model *history* beyond the credential's stored model.
- Writable `integration` CLI (OAuth connect stays browser-only).
- Stripe / billing surfaces (v2.1).
