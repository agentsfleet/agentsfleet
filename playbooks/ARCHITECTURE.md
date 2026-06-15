# Tunnel-First Deployment Architecture

**Status:** Architecture reference for agentsfleet's deployment topology.
**Scope:** Why every public-facing service in this project sits behind a Cloudflare Tunnel, what that buys, and what it costs.

---

## 1. The problem with every PaaS today

Railway, Render, Heroku, Fly.io (default config), and virtually every developer PaaS expose your origin server directly to the internet:

```
client → DNS → your server's public IP → app
```

This means:

- Your server's IP is discoverable — DDoS, scanning, probing
- You bolt on a CDN (Cloudflare) in front, but your origin IP leaks through headers, certificates, and DNS history
- A determined attacker bypasses Cloudflare and hits your origin directly
- You're responsible for rate limiting, TLS hardening, and DDoS absorption at the app layer
- Compliance and security teams flag "origin IP exposed" on every audit

That's the status quo. Most teams work around it; some don't.

---

## 2. What agentsfleet does instead

For agentsfleet's API deployment, every public service uses tunnel-first architecture:

```
client
  │
  ▼
Cloudflare Edge  (api-dev.agentsfleet.net / api.agentsfleet.net)
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
            │  agentsfleetd-dev.internal resolves to all API machines via DNS round-robin
            │
            ├──▶ agentsfleetd-dev machine 1 :3000
            └──▶ agentsfleetd-dev machine 2 :3000
```

**Key property:** `agentsfleetd-dev` has no public port. No public IP. No inbound firewall rules. It does not exist on the public internet. The only path in is through the tunnel.

---

## 3. How the tunnel works

The tunnel process (`cloudflared`) runs inside the private network and initiates outbound connections to Cloudflare's edge — similar to an SSH reverse tunnel, but production-grade and multiplexed.

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

Each connection uses HTTP/2 or QUIC — one TCP/UDP connection multiplexes thousands of concurrent request streams. 4 connections per `cloudflared` machine is not a throughput ceiling; it's a resilience number (4 distinct Cloudflare edge PoPs).

---

## 4. High availability — two independent layers

### Layer 1: cloudflared HA (Cloudflare side)

Two `cloudflared` machines each establish 4 connections → 8 total tunnel connections across different Cloudflare PoPs.

Cloudflare load balances incoming requests across all active connectors. If a machine dies, Cloudflare drains its connections and routes to the remaining ones. No config. No intervention.

### Layer 2: API HA (private network side)

`agentsfleetd-dev.internal:3000` resolves via Fly's internal DNS to all running API machines (IPv6, WireGuard). Each request from `cloudflared` gets DNS-resolved independently — natural round-robin load distribution with no LB in the path.

```
cloudflared
  ├──▶ agentsfleetd-dev.internal → DNS RR → machine 1 (fdaa::1:3000)
  └──▶ agentsfleetd-dev.internal → DNS RR → machine 2 (fdaa::2:3000)
```

Zero additional infrastructure. No load balancer process. No single point of failure.

---

## 5. Latency profile

```
Cloudflare Edge → cloudflared       ~5–15 ms   (tunnel overhead, persistent conn)
cloudflared → agentsfleetd-dev.internal  ~1–3 ms   (WireGuard, same region)
─────────────────────────────────────────────
Total overhead vs direct origin     ~6–18 ms
```

Acceptable for any API. The double-anycast trap (CF anycast → Fly anycast → app) is avoided by:

1. Running `cloudflared` **inside** the same private network as the API (Fly 6PN)
2. Targeting the `.internal` DNS name — bypasses Fly's public anycast LB entirely
3. Pinning `cloudflared` and `agentsfleetd-dev` to the **same region** (both `iad`)

Result: deterministic, low-latency routing with no re-routing surprises.

---

## 6. Service-to-service traffic (no tunnel needed)

Internal Fly traffic (Cloudflare connector → API; Fly Prometheus → `:9091` metrics) stays on the private 6PN mesh:

```
cloudflared-dev → agentsfleetd-dev.internal:3000  (Fly 6PN / WireGuard, never exits)
```

Only public ingress goes through the tunnel. The host-resident `agentsfleet-runner` is off-platform — it reaches the API over HTTPS carrying a runner token, not the 6PN mesh.

---

## 7. What this is NOT claiming

- This doesn't make the **app** secure — SQL injection, auth bugs, prompt injection, etc. are still application-level concerns
- Tunnel latency is real (~6–18 ms overhead) — not suitable for sub-millisecond latency requirements
- This is not a replacement for a proper WAF / security posture — it's one layer among many

---

## 8. Reference implementation

The architecture above is what agentsfleet's DEV + PROD deployments actually run.

| Component | Implementation |
|---|---|
| Compute (control plane / API) | Fly.io (`agentsfleetd-dev`, plus prod equivalent) |
| Compute (execution plane) | Host-resident `agentsfleet-runner` daemon on a bare-metal node (not Fly) — bootstrapped via `06_/07_runner_bootstrap_*` |
| Tunnel connector | Fly.io (`cloudflared-dev`, 2 machines; `cloudflared-prod`, 2 machines) |
| Edge | Cloudflare Tunnel (`agentsfleetd-dev` and `agentsfleetd-prod` tunnels) |
| Private network | Fly 6PN (WireGuard mesh) |
| DNS | `agentsfleetd-dev.internal`, `agentsfleetd-prod.internal` → all API machines |
| Public hostname | `api-dev.agentsfleet.net`, `api.agentsfleet.net` → tunnel CNAME |
| Public port on origin | None |
