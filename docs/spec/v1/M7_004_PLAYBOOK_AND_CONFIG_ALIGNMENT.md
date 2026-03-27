# M7_004: Playbook & Configuration Alignment Review

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 004
**Date:** Mar 26, 2026
**Status:** PENDING
**Priority:** P1 — Post-acceptance hygiene; ensures operator docs match shipped code
**Batch:** B4 — after M7_003 PROD Acceptance Gate
**Depends on:** M7_003 (PROD Acceptance Gate)

---

## 1.0 Configuration Documentation

**Status:** PENDING

Ensure `docs/CONFIGURATION.md` reflects the current runtime contract including renamed test env vars and all DB/Redis/auth variables.

**Dimensions:**
- 1.1 PENDING Update `docs/CONFIGURATION.md` with renamed test env vars: `TEST_DATABASE_URL`, `TEST_REDIS_TLS_URL`, `REDIS_TLS_CA_CERT_FILE`
- 1.2 PENDING Add core runtime env vars section: `DATABASE_URL_API`, `DATABASE_URL_WORKER`, `REDIS_URL_API`, `REDIS_URL_WORKER`, `REDIS_TLS_CA_CERT_FILE`
- 1.3 PENDING Add auth env vars section: `CLERK_SECRET_KEY`, `CLERK_PUBLISHABLE_KEY`, `OIDC_ISSUER`, `OIDC_JWKS_URL`
- 1.4 PENDING Add observability env vars: `LOG_LEVEL`, `POSTHOG_API_KEY`, `GRAFANA_OTLP_*`

---

## 2.0 Playbook Review

**Status:** PENDING

Cross-reference all playbooks in `playbooks/` against current code and deployed infrastructure. Flag stale steps, wrong env var names, or missing credentials.

**Dimensions:**
- 2.1 PENDING Review `playbooks/M2_002_PRIMING_INFRA.md` — verify Fly, Cloudflare, Redis, DB steps match current deploy
- 2.2 PENDING Review `playbooks/gates/check-credentials.sh` — verify all vault items referenced are current
- 2.3 PENDING Review `playbooks/M4_001_WORKER_BOOTSTRAP_DEV.md` — verify worker deploy matches baremetal/Tailscale setup
- 2.4 PENDING Verify `.env.example` or deploy docs reference correct env var names post-rename

---

## 3.0 Error Code Documentation

**Status:** PENDING

Create a reference page for all `UZ-*` error codes with hints, matching `errors/codes.zig`.

**Dimensions:**
- 3.1 PENDING Generate error code reference from `src/errors/codes.zig` (code → hint → docs URL)
- 3.2 PENDING Verify `ERROR_DOCS_BASE` URL (`https://docs.usezombie.com/error-codes#`) has a landing page or placeholder
- 3.3 PENDING Remove `REDIS_READY_TEST_URL` dead code from `redis_test.zig`

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `docs/CONFIGURATION.md` covers all runtime, test, and observability env vars
- [ ] 4.2 All playbooks in `playbooks/` reference correct env var names and vault items
- [ ] 4.3 Error code reference page exists at docs URL or has a placeholder
- [ ] 4.4 No stale env var names remain in source code or docs

---

## 5.0 Out of Scope

- New playbooks for features not yet shipped
- PROD deployment (→ M7_003)
- Security hardening (Upstash IP allowlist — separate workstream)
