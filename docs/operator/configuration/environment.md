# Environment variables

## Overview

UseZombie components are configured through environment variables. Each role (API, Worker, Executor) requires a different set of variables. On bare-metal deployments, variables are loaded from `/opt/zombie/.env` via the systemd `EnvironmentFile` directive.

UseZombie uses **role-separated database and Redis URLs**. The API and worker each get their own connection string with role-appropriate permissions.

## API server

Variables required by `zombied serve`.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL_API` | Yes | — | PostgreSQL connection string for the API role. |
| `REDIS_URL_API` | Yes | — | Redis connection string for the API role. |
| `CLERK_SECRET_KEY` | Yes | — | Clerk backend API secret key for JWT verification. |
| `ENCRYPTION_MASTER_KEY` | Yes | — | Master encryption key for secrets at rest. |
| `PORT` | No | `3000` | HTTP API listen port. |
| `API_HTTP_THREADS` | No | `1` | HTTP thread pool size. |
| `API_HTTP_WORKERS` | No | `1` | HTTP worker count. |
| `API_MAX_CLIENTS` | No | `1024` | Maximum concurrent client connections. |
| `API_MAX_IN_FLIGHT_REQUESTS` | No | `256` | Maximum in-flight request limit. |
| `RATE_LIMIT_CAPACITY` | No | `30` | Token bucket capacity for rate limiting. |
| `RATE_LIMIT_REFILL_PER_SEC` | No | `5.0` | Token bucket refill rate per second. |
| `DEFAULT_MAX_ATTEMPTS` | No | `3` | Default maximum run retry attempts. |
| `APP_URL` | No | `https://app.usezombie.com` | Application base URL for links. |
| `GIT_CACHE_ROOT` | No | `/tmp/zombie-git-cache` | Git bare repo cache directory. |

### OIDC (optional, alternative to Clerk)

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OIDC_PROVIDER` | No | — | OIDC provider name (enables OIDC auth path). |
| `OIDC_JWKS_URL` | No | — | JWKS endpoint URL for token verification. |
| `OIDC_ISSUER` | No | — | Expected token issuer. |
| `OIDC_AUDIENCE` | No | — | Expected token audience. |

## Worker

Variables required by `zombied worker`.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL_WORKER` | Yes | — | PostgreSQL connection string for the worker role. |
| `REDIS_URL_WORKER` | Yes | — | Redis connection string for work queue claims. |
| `SANDBOX_BACKEND` | No | `bubblewrap` (Linux), `host` (macOS) | Sandbox backend: `host` (no isolation) or `bubblewrap`. |
| `WORKER_CONCURRENCY` | No | `1` | Number of concurrent runs this worker can claim. |
| `RUN_TIMEOUT_MS` | No | `300000` | Maximum wall-clock time for a single run (5 min default). |
| `EXECUTOR_SOCKET_PATH` | No | `/run/zombie/executor.sock` | Path to the executor Unix socket. |
| `DRAIN_TIMEOUT_MS` | No | `270000` | Graceful shutdown timeout when draining active runs. |
| `EXECUTOR_STARTUP_TIMEOUT_MS` | No | `5000` | How long to wait for executor sidecar to become available. |
| `EXECUTOR_LEASE_TIMEOUT_MS` | No | `30000` | Executor lease validity period (heartbeat interval). |
| `GITHUB_APP_ID` | Yes | — | GitHub App ID for signing installation token JWTs. |
| `GITHUB_APP_PRIVATE_KEY` | Yes | — | GitHub App private key (PEM) for JWT signing. |

## Executor

Variables required by `zombied-executor`. The executor binary has no unique variables beyond the shared ones below.

<Info>
  Network policy (deny-by-default) is applied automatically by the executor's sandbox layer. There is no environment variable to configure it. The policy is hardcoded in `network.zig` and denies all egress by default.
</Info>

## Shared variables

These variables are used by both the Worker and Executor processes.

| Variable | Used by | Default | Description |
|----------|---------|---------|-------------|
| `SANDBOX_BACKEND` | Worker, Executor | `bubblewrap` (Linux), `host` (macOS) | Sandbox backend. Must match on both. |
| `EXECUTOR_MEMORY_LIMIT_MB` | Worker, Executor | `512` | Memory limit per agent execution in megabytes. |
| `EXECUTOR_CPU_LIMIT_PERCENT` | Worker, Executor | `100` | CPU limit as a percentage of one core. |
| `SANDBOX_KILL_GRACE_MS` | Worker, Executor | `250` | Grace period (ms) before force-killing a sandbox after timeout. |
| `LOG_LEVEL` | All | `info` | Log verbosity: `debug`, `info`, `warn`, `error`. |
| `REDIS_TLS_CA_CERT_FILE` | No | — | Path to CA certificate file for custom or self-signed TLS Redis connections. Must be absolute path. Required when `rediss://` URLs point to a Redis instance with a non-public CA. |

## Test environment variables

Variables used only during integration testing. These are not required for production runtime.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TEST_DATABASE_URL` | Test only | — | PostgreSQL connection string for DB-backed integration tests. |
| `TEST_REDIS_TLS_URL` | Test only | — | Redis TLS URL (rediss://) for testing TLS Redis connections. |

## Observability and telemetry

Variables for monitoring, analytics, and distributed tracing.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `POSTHOG_API_KEY` | No | — | PostHog project API key for product analytics. |
| `POSTHOG_HOST` | No | `https://us.i.posthog.com` | PostHog API endpoint. |
| `GRAFANA_OTLP_ENDPOINT` | No | — | Grafana OTLP endpoint for trace/metric export. |
| `GRAFANA_OTLP_INSTANCE_ID` | No | — | Grafana Cloud instance ID for authentication. |
| `GRAFANA_OTLP_API_KEY` | No | — | Grafana Cloud API key for OTLP export. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | No | — | Generic OTLP endpoint for OpenTelemetry. |
| `OTEL_EXPORTER_OTLP_HEADERS` | No | — | Headers for OTLP export (e.g., API keys). |
