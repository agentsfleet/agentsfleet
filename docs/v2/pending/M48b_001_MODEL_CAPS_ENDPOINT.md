# M48b_001: Model-Caps Endpoint — Public, Cryptic-Prefix Lookup

**Prototype:** v2.0.0
**Milestone:** M48b
**Workstream:** 001
**Date:** Apr 29, 2026
**Status:** PENDING
**Priority:** P1 — substrate. Both M48 (BYOK provider) and M49 (install-skill) consume this endpoint. Carved out of M48 to land independently because the data model + HTTP handler are simple, and the install-skill needs the URL stable before any BYOK CLI ships.
**Categories:** API, SCHEMA
**Batch:** B1 — substrate-tier alongside M40-M48.
**Branch:** Implemented inline on `feat/m41-context-layering` per operator authorization (auto-mode forward instruction); separate commit from the M41 substrate work.
**Depends on:** Nothing. Pure additive — new schema + new route, no existing code paths affected.

**Canonical architecture:** [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) §9 (endpoint shape, rotation, Cloudflare caching) and [`docs/architecture/scenarios/01_default_free_tier.md`](../../architecture/scenarios/01_default_free_tier.md) (consumer: install-skill at install time).

---

## Overview

A public, unauthenticated, cryptic-prefix endpoint that returns the model → context-window catalogue. Both `zombiectl provider set` (BYOK posture) and the `/usezombie-install-platform-ops` skill (platform posture) call this endpoint exactly once at provisioning time and pin the cap into the appropriate place (`core.tenant_providers.context_cap_tokens` or the generated SKILL.md frontmatter, respectively).

**Endpoint:** `GET /_um/<key>/model-caps.json[?model=<urlencoded>]`

The key is hard-coded in `zombiectl` and the install-skill. Quarterly rotation = coordinated CLI + skill release; old key serves `410 Gone` for thirty days, then `404`.

**Data source:** A new Postgres table `core.model_caps` seeded via the schema migration. Updates ship via new migrations (one row per model addition / cap correction) so that fresh installs and existing installs converge to the same catalogue.

**Why a table instead of a static JSON file:**

- Lets us evolve the catalogue without a binary release (e.g. via a future admin-zombie that opens migration PRs hourly).
- Keeps the handler shape identical regardless of where the data lives — the `models` array is built from a `SELECT`.
- Aligns with the project's "schema is the source of truth" pattern; ops people can audit the catalogue with `psql`.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/020_model_caps.sql` | NEW | Table + seed |
| `schema/embed.zig` | EXTEND | Register schema file |
| `src/cmd/common.zig` | EXTEND | Add migration to canonical array |
| `src/http/handlers/model_caps.zig` | NEW | Handler |
| `src/http/router.zig` | EXTEND | Route enum + path match |
| `src/http/route_table.zig` | EXTEND | Wire handler with no-auth middleware |
| `src/http/route_table_invoke.zig` | EXTEND | Invoke function |
| `src/http/handlers/model_caps_integration_test.zig` | NEW | Integration test against fresh-migrated DB |

---

## Sections

### §1 — Schema

`core.model_caps`:

- `model_id TEXT PRIMARY KEY` — provider-namespaced id; case-sensitive (`accounts/fireworks/models/kimi-k2.6` is distinct from `kimi-k2.6`).
- `context_cap_tokens INTEGER NOT NULL` — must be > 0 (validated in app code, not via SQL CHECK with literals — RULE STS).
- `created_at_ms BIGINT NOT NULL` — epoch milliseconds (project convention).
- `updated_at_ms BIGINT NOT NULL` — epoch milliseconds; max value across rows becomes the response's `version`.

Provider is intentionally not a column — see the seed-data table below.

Seed data inserted in the same migration:

| model_id | context_cap_tokens |
|---|---|
| `claude-opus-4-7` | 1000000 |
| `claude-sonnet-4-6` | 200000 |
| `claude-haiku-4-5-20251001` | 200000 |
| `gpt-5.5` | 256000 |
| `gpt-5.4` | 256000 |
| `kimi-k2.6` | 256000 |
| `accounts/fireworks/models/kimi-k2.6` | 256000 |
| `accounts/fireworks/models/deepseek-v4-pro` | 256000 |
| `glm-5.1` | 128000 |

The provider hosting a given model is encoded in the `model_id` itself:
`accounts/fireworks/models/...` → Fireworks; bare `kimi-k2.6` → Moonshot; `claude-*` → Anthropic; `gpt-*` → OpenAI; `glm-*` → Zhipu. Operators pick their provider via the `llm` credential body, not via this catalogue.

Caps reflect each model's documented context window at seed time. Updates to caps are new migration rows (`ON CONFLICT (model_id) DO UPDATE`) — never edit the original migration after release.

### §2 — Handler

Route key constant lives in the handler file (`MODEL_CAPS_PATH_KEY`). The router matches the literal path `/_um/<key>/model-caps.json`. Wrong-key requests return 404 (no leak about the right key's existence).

Behaviour:

- `GET /_um/<key>/model-caps.json` → 200 with `{ "version": "YYYY-MM-DD", "models": [...] }`. `version` is `MAX(updated_at_ms)` formatted as `YYYY-MM-DD`.
- `GET /_um/<key>/model-caps.json?model=<exact-id>` → 200 with the same envelope but `models` is at most one row. Unknown model → 200 with empty `models: []`. (No 404 — clients distinguish "unknown model" from "wrong key" by presence of the array.)
- Wrong key, missing key, wrong path → 404.
- Method other than GET → 405.
- DB unavailable → 503.

The handler runs without authentication and does not read or write any tenant data. It is safe to expose publicly; the cryptic prefix is for crawler-deflection only, not for security.

### §3 — Routing + auth

Wired through the existing M18_002 middleware pipeline with the `none` policy (same as `/healthz`). No new middleware, no auth-exempt list edit.

### §4 — Tests

| Test | Asserts |
|---|---|
| `test_returns_seed_catalogue` | GET with correct key returns `models` array with at least the seeded rows |
| `test_filter_by_known_model` | GET with `?model=claude-sonnet-4-6` returns exactly one row |
| `test_filter_by_unknown_model` | GET with `?model=does-not-exist` returns 200 + empty array (not 404) |
| `test_wrong_key_returns_404` | GET with `/_um/wrong-key/model-caps.json` returns 404 |
| `test_method_not_allowed` | POST returns 405 |

---

## Interfaces

```
HTTP:
  GET /_um/<key>/model-caps.json             → 200 { version, models: [...] }
  GET /_um/<key>/model-caps.json?model=X     → 200 { version, models: [0|1 rows] }
  GET /_um/wrong-key/model-caps.json         → 404
  POST /_um/<key>/model-caps.json            → 405

JSON shape:
  {
    "version": "2026-04-29",
    "models": [
      { "id": "claude-sonnet-4-6", "context_cap_tokens": 200000 },
      ...
    ]
  }

SQL:
  SELECT model_id, context_cap_tokens, updated_at_ms
    FROM core.model_caps
   [WHERE model_id = $1]
   ORDER BY model_id;
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| DB unreachable | Postgres outage | 503 with `{"error": "model_caps_unavailable"}` |
| Empty table | Migration didn't run | 503 with `{"error": "model_caps_unavailable"}` (treat empty as unhealthy — operators expect at least one row) |
| Wrong key | Crawler / stale client / rotation lag | 404 (same as missing-route — no leak) |
| Method other than GET | Misuse | 405 with `Allow: GET` |
| Path with query param other than `model=` | Misuse | Ignored; full catalogue returned |

---

## Invariants

1. **No auth.** The handler runs without a principal. It must not access `core.tenant_providers`, the vault, or any tenant data.
2. **No mutations.** Read-only. No `INSERT`/`UPDATE`/`DELETE`.
3. **No secrets in response.** The catalogue is public information; the response body is safe to log and cache.
4. **Cryptic key is constant for the lifetime of a release.** Rotation is a coordinated CLI + skill + API release — never silent.

---

## Acceptance Criteria

- [ ] `make test` passes (unit shape tests on the handler).
- [ ] `make test-integration` passes (the five tests in §4 against a fresh-migrated DB).
- [ ] `make check-pg-drain` clean.
- [ ] Cross-compile clean: `x86_64-linux` + `aarch64-linux`.
- [ ] Manual smoke: `curl https://api-dev.usezombie.com/_um/<key>/model-caps.json` returns the seeded catalogue.
- [ ] Manual smoke: `curl https://api-dev.usezombie.com/_um/wrong-key/model-caps.json` returns 404.

---

## Out of Scope

- Cloudflare cache rule (deployment configuration; lives elsewhere).
- Admin-zombie that auto-refreshes the catalogue (post-launch, separate spec).
- `zombiectl provider set` calling the endpoint (that's M48 §6).
- Install-skill calling the endpoint (that's M49).
- API for adding new models from the dashboard (post-launch).
- Per-region cap variants (Anthropic / OpenAI sometimes vary cap by region — out of scope; the seed value reflects the global default).
