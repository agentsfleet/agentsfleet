---
type: explanation
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Private API Ingress

## What it is

Cloudflare Tunnel is the only public path to `agentsfleetd`.
The Vercel dashboard and installer are separate surfaces.

```text
client → Cloudflare edge → cloudflared-<env> on Fly.io → agentsfleetd-<env>.internal:3000 → agentsfleetd
```

| Environment | Public API | Connector app | Private origin |
|---|---|---|---|
| Development | `api-dev.agentsfleet.net` | `cloudflared-dev` | `agentsfleetd-dev.internal:3000` |
| Production | `api.agentsfleet.net` | `cloudflared-prod` | `agentsfleetd-prod.internal:3000` |

## How it behaves

- The connector reaches `agentsfleetd` over Fly's private IPv6 network (6PN).
- The API `fly.toml` files declare neither `[http_service]` nor `[[services]]`, so Fly publishes no API origin.
- Every API and connector config sets `[[restart]] policy = "always"`.
- 6PN traffic skips Fly's public proxy, so the development workflow starts API machines before it polls the Cloudflare route.
- `/healthz` proves the process answers HTTP. `/readyz` gates on PostgreSQL and reports the NATS bus without gating on it, because every bus signal has a PostgreSQL backstop.
- Fly checks `/readyz` privately. Continuous Integration (CI) checks both endpoints through Cloudflare.
- `agentsfleet-runner` runs on its own host and calls the public API over HTTPS with a runner token.
- A runner holds no PostgreSQL or NATS credential and joins no datastore allowlist.

## Limits

The API image is distroless: the daemon, a certificate bundle and a clock.
`flyctl ssh console` gives no prompt there, and that is expected.
Diagnose from `flyctl logs` (logfmt), `/readyz` and `/healthz` through the tunnel, and the metrics families.

A question none of those answers needs a new emit from the daemon, not a shell.

## Related pages

- `deploy/fly/agentsfleetd-dev/fly.toml`, `deploy/fly/agentsfleetd-prod/fly.toml`
- `deploy/fly/cloudflared-dev/config.yml`, `deploy/fly/cloudflared-prod/config.yml`
- `.github/workflows/deploy-dev-verify.yml`, `.github/workflows/release.yml`
- [Infrastructure priming](./founding/03_priming_infra/001_playbook.md)
