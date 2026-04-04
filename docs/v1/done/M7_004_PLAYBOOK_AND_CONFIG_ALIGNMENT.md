# M7_004: Playbook & Configuration Alignment Review

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 004
**Date:** Mar 29, 2026
**Status:** DONE
**Priority:** P1 — Post-acceptance hygiene; ensures operator docs match shipped code
**Batch:** B4 — after M7_003 PROD Acceptance Gate
**Depends on:** M7_003 (PROD Acceptance Gate)

---

## 1.0 Configuration Documentation

**Status:** DONE

Updated `docs/operator/configuration/environment.md` to reflect the current runtime contract including renamed test env vars and all DB/Redis/auth variables.

**Dimensions:**
- 1.1 DONE Document renamed test env vars in environment.md: `TEST_DATABASE_URL`, `TEST_REDIS_TLS_URL`, `REDIS_TLS_CA_CERT_FILE`
- 1.2 DONE Core runtime env vars section exists: `DATABASE_URL_API`, `DATABASE_URL_WORKER`, `REDIS_URL_API`, `REDIS_URL_WORKER`, `REDIS_TLS_CA_CERT_FILE`
- 1.3 DONE Auth env vars section exists: `CLERK_SECRET_KEY`, `CLERK_PUBLISHABLE_KEY`, `OIDC_ISSUER`, `OIDC_JWKS_URL`
- 1.4 DONE Observability env vars documented: `LOG_LEVEL`, `POSTHOG_API_KEY`, `GRAFANA_OTLP_*`

---

## 2.0 Playbook Review

**Status:** DONE

Cross-referenced all playbooks in `playbooks/` against current code and deployed infrastructure.

**Dimensions:**
- 2.1 DONE Reviewed `playbooks/M2_002_PRIMING_INFRA.md` — Fly, Cloudflare, Redis, DB steps match current deploy
- 2.2 DONE Reviewed `playbooks/gates/check-credentials.sh` — redirects to `m2_001/run.sh`, vault items current
- 2.3 DONE Reviewed `playbooks/M4_001_WORKER_BOOTSTRAP_DEV.md` — worker deploy matches baremetal/Tailscale setup
- 2.4 DONE Verified `.env.example` references correct env var names post-rename

---

## 3.0 Error Code Documentation

**Status:** DONE

Error code reference maintained in `src/errors/codes.zig` with hints and docs URL base.

**Dimensions:**
- 3.1 DONE Error codes defined in `src/errors/codes.zig` with hints and `ERROR_DOCS_BASE` URL
- 3.2 DONE `ERROR_DOCS_BASE` set to `https://docs.usezombie.com/error-codes#` (placeholder acceptable)
- 3.3 DONE Removed `REDIS_READY_TEST_URL` dead code from `redis_test.zig:58-59`

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 `docs/operator/configuration/environment.md` covers all runtime, test, and observability env vars
- [x] 4.2 All playbooks in `playbooks/` reference correct env var names and vault items
- [x] 4.3 Error code reference page exists at docs URL or has a placeholder
- [x] 4.4 No stale env var names remain in source code or docs

---

## 5.0 Out of Scope

- New playbooks for features not yet shipped
- PROD deployment (→ M7_003)
- Security hardening (Upstash IP allowlist — separate workstream)
