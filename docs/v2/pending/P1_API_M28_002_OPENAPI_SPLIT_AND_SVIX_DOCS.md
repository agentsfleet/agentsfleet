# M28_002: Split `public/openapi.json` into per-domain parts + document Svix webhook endpoint

**Prototype:** v0.19.0
**Milestone:** M28
**Workstream:** 002
**Date:** Apr 18, 2026
**Status:** PENDING
**Priority:** P1 — unblock future API additions without monolithic file edits
**Batch:** B1
**Branch:** _unassigned_
**Depends on:** M28_001 (adds `/v1/webhooks/svix/{zombie_id}` route)

---

## Overview

**Goal (testable):** `public/openapi.json` is sourced from per-domain partial files assembled at build time. Adding or editing any single endpoint touches one small file, not the monolith. Any OpenAPI consumer (SDK generators, docs site) still reads a single assembled `public/openapi.json` — the split is source-only, not output.

**Problem:** `public/openapi.json` has grown large enough that any endpoint addition consumes heavy editing tokens and risks merge conflicts. M28_001 added the new `/v1/webhooks/svix/{zombie_id}` route but intentionally did **not** update openapi.json, deferring it here so the split + endpoint docs land together.

**Solution summary:**
- Split `public/openapi.json` into `public/openapi/{info,components,webhooks,zombies,workspaces,admin,...}.json` partials.
- Add a `make openapi-assemble` (or Zig build step) that concatenates partials into `public/openapi.json`.
- CI verifies the assembled output is in sync with the partials (fail if drifted).
- Document the Svix webhook endpoint (`POST /v1/webhooks/svix/{zombie_id}`) and the minimal `signature.secret_ref` TRIGGER.md contract as part of the initial split.

---

## Sections (implementation slices)

### §1 — Partial layout and assembly tool

Define the partial directory structure, pick the assembly tool (Zig build step preferred — keeps toolchain uniform), write the assembler, emit the single `public/openapi.json` identical to today's content (byte-for-byte diff == 0 as the gate).

### §2 — Document the Svix webhook route

Add the `/v1/webhooks/svix/{zombie_id}` entry to the webhook partial:
- `POST` only, 202 on valid signature, 401 on auth failure (`UZ-WH-010` / `UZ-WH-011`).
- Path param `zombie_id` (UUIDv7).
- Request body: provider-shaped JSON (opaque — documented as free-form payload forwarded to the zombie handler).
- Headers documented: `svix-id`, `svix-timestamp`, `svix-signature`.
- TRIGGER.md contract example in the description.

### §3 — Register the route in the OpenAPI + CI gate

Wire the assembler into CI; add a check that rejects any PR where `public/openapi.json` content drifts from the assembled partials.

---

## Acceptance Criteria

- [ ] `public/openapi.json` is assembled from per-domain partials and byte-matches the pre-split file (after the initial split commit).
- [ ] `/v1/webhooks/svix/{zombie_id}` documented with full request/response schemas.
- [ ] CI fails on `public/openapi.json` drift vs partials.
- [ ] `make openapi-assemble` is idempotent.

---

## Out of Scope

- Splitting the docs site content (separate repo, separate milestone).
- Changes to the Svix middleware or route behavior — those are M28_001.
- OpenAPI 3.1 upgrade — stays on 3.0 until a dedicated milestone.
