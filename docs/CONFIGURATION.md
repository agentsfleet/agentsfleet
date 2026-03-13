# Runtime Environment Contract

Date: Mar 07, 2026
Status: Active
Owner workstream: `docs/done/v1/M4_007_RUNTIME_ENV_CONTRACT.md`

## Goal

Define one canonical environment-variable contract for `zombied` and `zombiectl` so startup, operations, and troubleshooting remain deterministic.

## Precedence Policy

- Security-critical values are env-only.
- Non-secret operator ergonomics may use CLI overrides.
- Allowed override order for supported knobs: `CLI flag > process env > .env.local (dev fallback, non-overriding) > default`.
- `.env.local` is loaded only in dev-friendly mode (`ZOMBIED_ENV_MODE=dev`, `ZOMBIED_LOAD_DOTENV=true`, or Debug build default) and never overrides existing process env.

Current expectation:
- `zombied`: env-first runtime.
- `zombiectl`: keep slim; only non-secret convenience flags should override env.

Current implemented non-secret CLI override:
- `zombied serve --port <u16>` (equivalent to overriding `PORT`).

## Required Runtime Keys

These must be present for secure `zombied serve` operation.

- `ENCRYPTION_MASTER_KEY`: required, exactly 64 hex chars.
- `GITHUB_APP_ID`: required, non-empty.
- `GITHUB_APP_PRIVATE_KEY`: required, non-empty.
- `DATABASE_URL_API`: required, non-empty.
- `DATABASE_URL_WORKER`: required, non-empty and different from `DATABASE_URL_API`.
- `REDIS_URL_API`: required, must use `rediss://`.
- `REDIS_URL_WORKER`: required, must use `rediss://` and differ from `REDIS_URL_API`.

Auth mode:
- OIDC enabled (`OIDC_JWKS_URL` set and non-empty): `OIDC_PROVIDER` may be `clerk` or `custom`.
- API key auth enabled (`API_KEY` set and non-empty): bearer API key auth is accepted as a separate user auth type.
- At least one auth path must be configured: `OIDC_JWKS_URL` or `API_KEY`.

## Configuration Partitions

Required column legend:
- `Yes`: always required for secure runtime startup.
- `Conditional`: required only when a specific mode is enabled.
- `Optional`: not required for core startup; used when that feature is enabled.

### Auth

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `OIDC_PROVIDER` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Defaults to `clerk`; supported values: `clerk`, `custom`. |
| `OIDC_JWKS_URL` | Conditional | Process env (optional dev `.env.local` fallback), no CLI | Required when any `OIDC_*` setting is used. Empty values fail startup. |
| `OIDC_ISSUER` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Expected issuer check for the active provider. |
| `OIDC_AUDIENCE` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Expected audience check for the active provider. |
| `API_KEY` | Conditional | Process env (optional dev `.env.local` fallback), no CLI | Enables bearer API key auth as a separate user auth type. |
| `GITHUB_APP_ID` | Yes | Process env (optional dev `.env.local` fallback), no CLI | GitHub App runtime auth. |
| `GITHUB_APP_PRIVATE_KEY` | Yes | Process env (optional dev `.env.local` fallback), no CLI | GitHub App runtime auth. |

### Server

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `PORT` | Optional | CLI `--port` > process env > optional dev `.env.local` > default | `zombied serve --port <u16>` can override. |
| `API_HTTP_THREADS` | Optional | Process env > optional dev `.env.local` > default | Server tuning knob. |
| `API_HTTP_WORKERS` | Optional | Process env > optional dev `.env.local` > default | Server tuning knob. |
| `API_MAX_CLIENTS` | Optional | Process env > optional dev `.env.local` > default | Server tuning knob. |
| `API_MAX_IN_FLIGHT_REQUESTS` | Optional | Process env > optional dev `.env.local` > default | Server tuning knob. |

### Worker And Readiness

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `WORKER_CONCURRENCY` | Optional | Process env > optional dev `.env.local` > default | Must be `> 0`. |
| `DEFAULT_MAX_ATTEMPTS` | Optional | Process env > optional dev `.env.local` > default | Must be `> 0`. |
| `RUN_TIMEOUT_MS` | Optional | Process env > optional dev `.env.local` > default | Must be `> 0`. |
| `RATE_LIMIT_CAPACITY` | Optional | Process env > optional dev `.env.local` > default | Must be `> 0`. |
| `RATE_LIMIT_REFILL_PER_SEC` | Optional | Process env > optional dev `.env.local` > default | Must be `> 0`. |
| `READY_MAX_QUEUE_DEPTH` | Optional | Process env > optional dev `.env.local` > default | If set, must be `> 0`. |
| `READY_MAX_QUEUE_AGE_MS` | Optional | Process env > optional dev `.env.local` > default | If set, must be `> 0`. |

### Storage

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `DATABASE_URL_API` | Yes | Process env (optional dev `.env.local` fallback), no CLI | Must be non-empty. |
| `DATABASE_URL_WORKER` | Yes | Process env (optional dev `.env.local` fallback), no CLI | Must be non-empty and differ from API DB URL. |
| `REDIS_URL_API` | Yes | Process env (optional dev `.env.local` fallback), no CLI | Must use `rediss://`. |
| `REDIS_URL_WORKER` | Yes | Process env (optional dev `.env.local` fallback), no CLI | Must use `rediss://` and differ from API Redis URL. |
| `REDIS_TLS_CA_CERT_FILE` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Needed for local/self-signed TLS Redis. |

### Secrets

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `ENCRYPTION_MASTER_KEY` | Yes | Process env (optional dev `.env.local` fallback), no CLI | Must be exactly 64 hex chars. |

### Observability

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `LOG_LEVEL` | Optional | Process env > optional dev `.env.local` > default | Process-env log level. |
| `POSTHOG_API_KEY` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Required when PostHog is enabled in zombied. |
| `POSTHOG_HOST` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Used by non-zombied surfaces; zombied defaults to `https://us.i.posthog.com`. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Optional | Process env (optional dev `.env.local` fallback), no CLI | OTEL endpoint (when enabled). |
| `OTEL_EXPORTER_OTLP_HEADERS` | Optional | Process env (optional dev `.env.local` fallback), no CLI | OTEL headers (when enabled). |

### Notifications

| Key | Required | Override Source | Notes |
|---|---|---|---|
| `DISCORD_WEBHOOK_URL` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Required when Discord notifications are enabled. |
| `SLACK_WEBHOOK_URL` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Required when Slack notifications are enabled. |
| `RESEND_API_KEY` | Optional | Process env (optional dev `.env.local` fallback), no CLI | Required when email notifications are enabled. |

## End-User `.env` Policy (Design Decision)

This project treats `.env.local` as a **developer convenience fallback only**. It is not a production source of truth.

Rules:
- Process env always wins over `.env.local`.
- `.env.local` is loaded only in dev-friendly mode (see Precedence Policy).
- `.env.local` must never be committed.

### Keys That Must Not Be Used As Runtime Fallback Contract

These keys are not part of the secure role-separated runtime contract for `zombied serve` / `zombied worker`:
- `DATABASE_URL` (use `DATABASE_URL_API` and `DATABASE_URL_WORKER`)
- `REDIS_URL` (use `REDIS_URL_API` and `REDIS_URL_WORKER`)

Operator expectation:
- If only `DATABASE_URL`/`REDIS_URL` is set, contract validation should be treated as misconfigured for serve/worker operation.

### Keys That Should Never Come From CLI Flags

Security-critical values are env-only and must not be introduced as CLI flags:
- `ENCRYPTION_MASTER_KEY`
- `GITHUB_APP_ID`
- `GITHUB_APP_PRIVATE_KEY`
- `OIDC_PROVIDER`
- `OIDC_JWKS_URL`
- `OIDC_ISSUER`
- `OIDC_AUDIENCE`
- `API_KEY`
- `DATABASE_URL_API`
- `DATABASE_URL_WORKER`
- `REDIS_URL_API`
- `REDIS_URL_WORKER`

Only non-secret ergonomics (example: `--port`) are valid CLI override candidates.

## Runtime Knobs

Validated at startup:
- `PORT` (default `3000`)
- `DEFAULT_MAX_ATTEMPTS` (default `3`, must be `> 0`)
- `WORKER_CONCURRENCY` (default `1`, must be `> 0`)
- `RUN_TIMEOUT_MS` (default `300000`, must be `> 0`)
- `RATE_LIMIT_CAPACITY` (default `30`, must be `> 0`)
- `RATE_LIMIT_REFILL_PER_SEC` (default `5.0`, must be `> 0`)
- `READY_MAX_QUEUE_DEPTH` (optional, if set must be `> 0`)
- `READY_MAX_QUEUE_AGE_MS` (optional, if set must be `> 0`)

## Doctor Contract

`zombied doctor` must deterministically validate:
- role-separated DB/Redis env presence and non-equality
- Redis TLS scheme (`rediss://`)
- API and worker DB config load
- Redis readiness and ACL identity checks
- required secrets (`ENCRYPTION_MASTER_KEY`, GitHub App keys)
- auth reachability (`OIDC_JWKS_URL` when OIDC is enabled)

Output contract:
- Human mode: `zombied doctor` (default)
- Machine mode: `zombied doctor --format=json` (or `--json`)

Output must stay stable for CI/operator automation.

## Canonical Trace Context

For telemetry payloads/events, standardize these fields:
- `trace_id`
- `span_id`
- `request_id`

`correlation_id` is optional, only for cross-trace joins (for example, linking one user action to multiple traces).

## Rotation Terms

- Key-versioned envelope metadata: encrypted records include the key version used for encryption (`kek_version`).
- No-downtime rotation: dual-read/single-write behavior during key rollout so traffic continues uninterrupted.

## Drift Rule

When behavior changes:
1. Update this file first.
2. Update `.env.example` and deployment docs in the same change.
3. Ensure `zombied serve` and `zombied doctor` validations still match documented contract.
