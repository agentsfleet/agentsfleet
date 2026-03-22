# Tunnel-First Deployment Architecture
# A Case for Origin-Shielded PaaS

**Author:** Internal — usezombie infra notes
**Date:** Mar 20, 2026
**Status:** Architecture reference + startup thesis

---

## 1. The Problem With Every PaaS Today

Railway, Render, Heroku, Fly.io (default config), and virtually every developer PaaS expose your origin server directly to the internet.

```
client → DNS → your server's public IP → app
```

This means:

- Your server's IP is discoverable — DDoS, scanning, probing
- You bolt on a CDN (Cloudflare) in front, but your origin IP leaks through headers, certificates, and DNS history
- A determined attacker bypasses Cloudflare and hits your origin directly
- You're responsible for rate limiting, TLS hardening, and DDoS absorption at the app layer
- Compliance and security teams flag "origin IP exposed" on every audit

This is the status quo. Every team works around it with varying degrees of success. Most don't.

---

## 2. What We're Building (usezombie infra today)

For usezombie's API deployment, we solved this with a tunnel-first architecture:

```
client
  │
  ▼
Cloudflare Edge  (api-dev.usezombie.com / api.usezombie.com)
  │
  │  Cloudflare Tunnel
  │  — encrypted, persistent, outbound-initiated
  │  — 8 connections total (2 cloudflared machines × 4 connections each)
  │  — each connection lands on a different Cloudflare PoP
  │
  ├──▶ cloudflared machine 1  (Fly app: cloudflared-dev, region: iad)
  └──▶ cloudflared machine 2  (Fly app: cloudflared-dev, region: iad)
            │
            │  Fly 6PN (WireGuard mesh — internal only, never public internet)
            │  zombied-dev.internal resolves to all API machines via DNS round-robin
            │
            ├──▶ zombied-dev machine 1 :3000
            └──▶ zombied-dev machine 2 :3000
```

**The key property:** `zombied-dev` has no public port. No public IP. No inbound firewall rules. It does not exist on the internet. The only path in is through the tunnel.

---

## 3. How the Tunnel Works

The tunnel process (`cloudflared`) runs inside your private network and initiates outbound connections to Cloudflare's edge — similar to how an SSH reverse tunnel works, but production-grade and multiplexed.

```
Private network              Cloudflare Edge
cloudflared ──outbound──▶   edge server 1  ←── client request
                         ←── stream proxied back over same connection
```

Because it's outbound-initiated:
- No inbound firewall rules needed
- No public IP on the origin
- No port forwarding
- Works behind NAT

Each connection uses **HTTP/2 or QUIC** — so one TCP/UDP connection multiplexes thousands of concurrent request streams. 4 connections per `cloudflared` machine is not a throughput ceiling — it's a resilience number (4 distinct Cloudflare edge PoPs).

---

## 4. High Availability — Two Independent Layers

### Layer 1: cloudflared HA (Cloudflare side)

Two `cloudflared` machines each establish 4 connections → 8 total tunnel connections across different Cloudflare PoPs.

Cloudflare load balances incoming requests across all active connectors. If a machine dies, Cloudflare drains its connections and routes to the remaining ones. No config. No intervention.

### Layer 2: API HA (private network side)

`zombied-dev.internal:3000` resolves via Fly's internal DNS to all running API machines (IPv6, WireGuard). Each request from `cloudflared` gets DNS-resolved independently — natural round-robin load distribution with no LB in the path.

```
cloudflared
  ├──▶ zombied-dev.internal → DNS RR → machine 1 (fdaa::1:3000)
  └──▶ zombied-dev.internal → DNS RR → machine 2 (fdaa::2:3000)
```

Zero additional infrastructure. No load balancer process. No single point of failure.

---

## 5. Latency Profile

```
Cloudflare Edge → cloudflared    ~5–15 ms   (tunnel overhead, persistent conn)
cloudflared → zombied-dev.internal  ~1–3 ms   (WireGuard, same region)
─────────────────────────────────────────────
Total overhead vs direct origin  ~6–18 ms
```

This is acceptable for any API. The double-anycast trap (CF anycast → Fly anycast → app) is avoided by:

1. Running `cloudflared` **inside** the same private network as the API (Fly 6PN)
2. Targeting the `.internal` DNS name — bypasses Fly's public anycast LB entirely
3. Pinning `cloudflared` and `zombied-dev` to the **same region** (both `iad`)

Result: deterministic, low-latency routing with no re-routing surprises.

---

## 6. The Startup Thesis

### What every PaaS is missing

Railway, Render, Heroku give you:
- Git push → deploy ✅
- Managed databases ✅
- Preview environments ✅
- **Origin shielding** ❌
- **Tunnel-first networking** ❌
- **No public IP by default** ❌

Cloudflare gives you tunnel primitives but doesn't manage your deployment. Fly gives you private networking but exposes a public port by default. Nobody ships the whole thing wired together as a first-class deployment model.

### The product

**A PaaS where "no public IP" is the default, not an advanced config.**

Every app deployed gets:

1. A Cloudflare Tunnel provisioned automatically at deploy time
2. Zero public ports on the compute layer — private network only
3. Edge routing via Cloudflare (DDoS, WAF, rate limiting included)
4. HA tunnel connectors managed by the platform
5. Internal service mesh for service-to-service (worker → API, etc.)

Developer experience:
```bash
# Today (Railway / Render / Heroku)
git push → app deployed → public IP exposed → you figure out the rest

# Tunnel-first PaaS
git push → app deployed → tunnel provisioned → no public IP, ever
           dev.api.yourapp.com live in 60s, origin shielded by default
```

### Why this wins

| | Traditional PaaS | Tunnel-First PaaS |
|---|---|---|
| Origin IP exposed | Yes | Never |
| DDoS surface | Origin + edge | Edge only |
| Cloudflare bypass possible | Yes | No (no public IP to bypass to) |
| Compliance posture | Audit finding | Clean |
| HA tunnel | DIY | Managed |
| Latency overhead | 0 ms (but unshielded) | ~10–18 ms (shielded) |
| Config required | Lots | Zero |

### Who buys this

- **Security-conscious SaaS teams** — compliance requires origin shielding
- **Fintech / healthtech / legaltech** — regulated industries, audit pressure
- **API-first startups** — want Cloudflare's network without the integration work
- **Enterprises** moving off on-prem — want VPC-equivalent on PaaS

### Competitive moat

- Railway, Render, Heroku cannot easily retro-fit this — their networking model exposes public IPs by design
- Cloudflare could build it but they're an edge/network company, not a compute/PaaS company
- Fly.io has the primitives but not the product — no tunnel-first defaults, no managed `cloudflared`
- The moat is the **wired-together defaults**, not any single piece of tech

---

## 7. How to Build It (Technical Sketch)

### Core loop per deploy

```
1. git push / image push
2. Platform provisions compute (Fly machines, Nomad, k8s — doesn't matter)
   → no [http_service] block / no NodePort / no LoadBalancer
3. Platform calls Cloudflare API: create tunnel for this app
4. Platform deploys cloudflared as a sidecar or dedicated app, same region
   → cloudflared config: service = http://app.internal:<port>
5. Platform calls Cloudflare API: route dns <tunnel-id> <hostname>
6. Deploy complete. Public URL live. Origin never touched public internet.
```

### Multiplexing and throughput

- Each `cloudflared` machine: 4 connections × HTTP/2 streams = effectively unlimited for API workloads
- Scale throughput: add `cloudflared` machines (platform manages this automatically)
- Scale compute: add app machines (`.internal` DNS RR distributes automatically)

### Multi-region

```
iad region:
  cloudflared-iad (2 machines) → app-iad.internal:3000

fra region:
  cloudflared-fra (2 machines) → app-fra.internal:3000

Cloudflare routes each user to nearest PoP → nearest tunnel connector → nearest compute
```

Platform manages regional tunnel topology. Developer deploys once, specifies regions.

### Service-to-service (no tunnel needed)

Internal traffic (worker → API) stays on the private mesh:
```
worker.internal → api.internal:3000  (WireGuard, never exits)
```

Only public ingress goes through the tunnel. Internal traffic is zero-overhead.

---

## 8. What We're Not Claiming

- This doesn't make your **app** secure — SQL injection, auth bugs, etc. are still your problem
- Tunnel latency is real (~10–18 ms overhead) — not suitable for sub-millisecond latency requirements
- This is not a replacement for a proper WAF/security posture — it's one layer

---

## 9. Reference Implementation (usezombie)

This document is derived from the actual usezombie DEV + PROD deployment architecture.

| Component | Implementation |
|---|---|
| Compute | Fly.io (`zombied-dev`, `zombied-dev-worker`) |
| Tunnel connector | Fly.io (`cloudflared-dev`, 2 machines) |
| Edge | Cloudflare Tunnel (`zombied-dev` tunnel) |
| Private network | Fly 6PN (WireGuard mesh) |
| DNS | `zombied-dev.internal` → all API machines |
| Public hostname | `api-dev.usezombie.com` → tunnel CNAME |
| Public port on origin | None |

Playbook: `playbooks/M2_002_PRIMING_INFRA.md §2.0`
