# M7_005: Network Connectivity — Tunnel, Database, and Cache Access

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 005
**Date:** Mar 27, 2026
**Status:** DONE
**Branch:** v0.3.1-prerelease-readiness
**Priority:** P0 — Services cannot operate without network connectivity
**Batch:** B1 — DEV first, then PROD
**Depends on:** M7_001 (DEV infra gate green)

---

## 1.0 DEV — Cloudflare Tunnel → Fly.io API

**Status:** DONE

Cloudflare Tunnel `zombied-dev` is the only ingress path to the Fly.io API. No public Fly ports are exposed. The tunnel must route `api-dev.usezombie.com` to the Fly private network address `zombied-dev.internal:3000`.

**Dimensions:**
- 1.1 DONE `cloudflared-dev` Fly app is running, tunnel status is `healthy` in Cloudflare dashboard
- 1.2 DONE Cloudflare DNS CNAME `api-dev.usezombie.com` resolves to tunnel CNAME (not a direct IP)
- 1.3 DONE Tunnel ingress rule routes `api-dev.usezombie.com` → `http://zombied-dev.internal:3000`
- 1.4 DONE End-to-end: `curl -sf https://api-dev.usezombie.com/healthz` returns 200 from outside the Fly network

**Evidence (Apr 02, 2026):**
```
$ curl -sf https://api-dev.usezombie.com/healthz
{"status":"ok","service":"zombied","database":"up","commit":"f5f3194"}
```

```bash
# Verify tunnel routing
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'

# Verify DNS
dig CNAME api-dev.usezombie.com +short
```

---

## 2.0 DEV — PlanetScale Access from Fly API and Workers

**Status:** DONE

PlanetScale DEV database must be reachable from both the Fly.io API machine and the bare-metal worker nodes. PlanetScale uses TLS-only connections over the public internet — no IP allow-listing required, but connection strings must be correct and credentials valid.

**Dimensions:**
- 2.1 DONE Fly.io `zombied-dev` can connect to PlanetScale DEV (verified by `/readyz` returning `"database": true`)
- 2.2 DONE Bare-metal worker `zombie-dev-worker-ant` can connect to PlanetScale DEV (worker readyz `worker: true`)
- 2.3 DONE Vault items verified: `planetscale-dev/api-connection-string`, `planetscale-dev/worker-connection-string`, `planetscale-dev/migrator-connection-string`

**Evidence (Apr 02, 2026):**
```
$ curl -sf https://api-dev.usezombie.com/readyz | jq '.database'
true
```

```bash
# Verify from Fly (via readyz)
curl -sf https://api-dev.usezombie.com/readyz | jq '.database'

# Verify vault items exist
op read "op://ZMB_CD_DEV/planetscale-dev/api-connection-string" > /dev/null && echo "ok"
```

---

## 3.0 DEV — Redis (Upstash) Access from Fly API and Workers

**Status:** DONE

Upstash DEV Redis must be reachable from both the Fly.io API machine and the bare-metal worker nodes. Upstash uses TLS connections over the public internet.

**Dimensions:**
- 3.1 DONE Fly.io `zombied-dev` can connect to Upstash DEV Redis (verified by `/readyz` returning `"queue_dependency": true`)
- 3.2 DONE Bare-metal worker `zombie-dev-worker-ant` can connect to Upstash DEV Redis (worker readyz `worker: true`)
- 3.3 DONE Vault items verified: `upstash-dev/api-url`, `upstash-dev/worker-url`

**Evidence (Apr 02, 2026):**
```
$ curl -sf https://api-dev.usezombie.com/readyz | jq '.queue_dependency'
true
```

```bash
# Verify from Fly (via readyz)
curl -sf https://api-dev.usezombie.com/readyz | jq '.queue_dependency'

# Verify vault items exist
op read "op://ZMB_CD_DEV/upstash-dev/api-url" > /dev/null && echo "ok"
op read "op://ZMB_CD_DEV/upstash-dev/worker-url" > /dev/null && echo "ok"
```

---

## 4.0 PROD — Cloudflare Tunnel → Fly.io API

**Status:** DONE

Same pattern as DEV. Tunnel `zombied-prod` routes `api.usezombie.com` → `zombied-prod.internal:3000`. Dependency for M7_003 §5.0.

**Dimensions:**
- 4.1 DONE `cloudflared-prod` Fly app running, tunnel healthy
- 4.2 DONE Cloudflare DNS CNAME `api.usezombie.com` resolves to tunnel CNAME
- 4.3 DONE Tunnel ingress routes `api.usezombie.com` → `http://zombied-prod.internal:3000`
- 4.4 DONE End-to-end: `curl -sf https://api.usezombie.com/healthz` returns 200

**Evidence (Apr 04, 2026):**
```
$ curl -sf https://api.usezombie.com/healthz
{"status":"ok","service":"zombied","database":"up","commit":"7eee364"}
```

---

## 5.0 PROD — PlanetScale Access from Fly API and Workers

**Status:** DONE

Dependency for M7_003 §5.4 and §6.3.

**Dimensions:**
- 5.1 DONE Fly.io `zombied-prod` can connect to PlanetScale PROD (verified by `/readyz` returning `"database": true`)
- 5.2 DONE Bare-metal workers `zombie-prod-worker-ant` and `zombie-prod-worker-bird` can connect to PlanetScale PROD (worker readyz `worker: true`)
- 5.3 DONE Vault items verified: `planetscale-prod/api-connection-string`, `planetscale-prod/worker-connection-string`, `planetscale-prod/migrator-connection-string`

**Evidence (Apr 04, 2026):**
```
$ curl -sf https://api.usezombie.com/readyz | jq '.database'
true
```

---

## 6.0 PROD — Redis (Upstash) Access from Fly API and Workers

**Status:** DONE

Dependency for M7_003 §5.4 and §6.3.

**Dimensions:**
- 6.1 DONE Fly.io `zombied-prod` can connect to Upstash PROD Redis (verified by `/readyz` returning `"queue_dependency": true`)
- 6.2 DONE Bare-metal workers can connect to Upstash PROD Redis (worker readyz `worker: true`)
- 6.3 DONE Vault items verified: `upstash-prod/api-url`, `upstash-prod/worker-url`

**Evidence (Apr 04, 2026):**
```
$ curl -sf https://api.usezombie.com/readyz | jq '.queue_dependency'
true
```

---

## 7.0 Acceptance Criteria

**Status:** DONE

- [x] 7.1 DEV: `api-dev.usezombie.com/healthz` returns 200 (tunnel → Fly → app)
- [x] 7.2 DEV: `api-dev.usezombie.com/readyz` returns `database: true` and `queue_dependency: true`
- [x] 7.3 DEV: bare-metal worker connects to PlanetScale and Redis (service starts, consumes queue)
- [x] 7.4 PROD: `api.usezombie.com/healthz` returns 200
- [x] 7.5 PROD: `api.usezombie.com/readyz` returns `database: true` and `queue_dependency: true`
- [x] 7.6 PROD: both worker nodes connect to PlanetScale and Redis

---

## 8.0 Out of Scope

- Cloudflare WAF / rate limiting rules (post-launch hardening)
- PlanetScale IP allow-listing (uses TLS auth, not IP-based)
- Redis cluster failover testing
- VPN or WireGuard mesh (Tailscale handles worker connectivity)
