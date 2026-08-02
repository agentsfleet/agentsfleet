# Private API Ingress

This is the supported ingress topology for the `agentsfleetd` API. The
Vercel-hosted dashboard and installer are separate surfaces.

```text
client
  → Cloudflare edge
  → outbound Cloudflare Tunnel connector on Fly.io
  → agentsfleetd-<env>.internal:3000
  → agentsfleetd
```

The connector reaches `agentsfleetd` over Fly's private Internet Protocol
version 6 network (6PN). The API apps define neither `[http_service]` nor
`[[services]]`, so Fly does not publish an API origin.

| Environment | Public API target | Connector app | Private origin |
|---|---|---|---|
| Development | `api-dev.agentsfleet.net` | `cloudflared-dev` | `agentsfleetd-dev.internal:3000` |
| Production | `api.agentsfleet.net` | `cloudflared-prod` | `agentsfleetd-prod.internal:3000` |

## Health behavior

- `/healthz` proves the process can answer HTTP.
- `/readyz` proves PostgreSQL and Redis are reachable.
- Fly checks `/readyz` over the private network.
- Continuous Integration (CI) checks both endpoints through the public
  Cloudflare route.

Private 6PN traffic does not use Fly's public proxy. The development workflow
therefore starts API machines explicitly before polling the Cloudflare route.
All API and connector configs use an always-restart rule.

## Boundaries

- Cloudflare Tunnel is the only supported public path to `agentsfleetd`.
- `agentsfleet-runner` runs on a separate host and calls the public API over
  HTTPS with a runner token.
- A runner has no PostgreSQL or Redis credential and does not join datastore
  allowlists.
- A hostname in a playbook is target state. Only the verification steps prove
  that its DNS and route are live.

## Checked-in sources

- `deploy/fly/agentsfleetd-dev/fly.toml`
- `deploy/fly/agentsfleetd-prod/fly.toml`
- `deploy/fly/cloudflared-dev/config.yml`
- `deploy/fly/cloudflared-prod/config.yml`
- `.github/workflows/deploy-dev.yml`
- `.github/workflows/release.yml`
- `playbooks/founding/03_priming_infra/001_playbook.md`
