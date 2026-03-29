# API server

## Overview

The API server is started with `zombied serve`. It listens for HTTP requests on port 3000 and exposes Prometheus metrics on port 9091. The server handles authentication, spec validation, run lifecycle, workspace management, and webhook delivery.

## Network topology

The API runs on Fly.io inside a Docker container. It is **not** exposed on a public `*.fly.dev` address. Instead, a Cloudflare Tunnel connects the Fly.io instance to Cloudflare's edge network.

- **HTTPS termination** happens at the Cloudflare edge.
- **Internal traffic** between Cloudflare Tunnel and the Fly.io container is plain HTTP.
- All public traffic routes through `api.usezombie.com` on the Cloudflare edge.

```
Client --> Cloudflare Edge (HTTPS) --> Cloudflare Tunnel --> zombied serve (HTTP :3000)
```

## Deployment

The API is deployed as a Docker container on Fly.io.

```dockerfile
FROM debian:bookworm-slim
COPY zombied /usr/local/bin/zombied
EXPOSE 3000 9091
CMD ["zombied", "serve"]
```

Deploy with the Fly CLI:

```bash
fly deploy --app zombied-api
```

## Ports

| Port | Purpose |
|------|---------|
| `3000` | HTTP API (default, configurable via `PORT`) |
| `9091` | Prometheus metrics (configurable via `METRICS_PORT`) |

## Health checks

The API exposes two health endpoints:

| Endpoint | Purpose |
|----------|---------|
| `GET /healthz` | Liveness probe — returns `200` if the process is running. |
| `GET /readyz` | Readiness probe — returns `200` if PostgreSQL and Redis are reachable. |

## Startup checks

On boot, `zombied serve` validates that all required environment variables are set and that it can connect to PostgreSQL and Redis. If any check fails, the process exits with a non-zero code and a diagnostic message naming the missing dependency.

See [Environment variables](/operator/configuration/environment) for the full list of API configuration.
