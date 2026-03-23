# Postgres Security

## Why This Exists

Postgres stores both control-plane state and encrypted secret material (`vault` schema). Role collapse would allow unauthorized reads of secret rows.

## Decisions

1. Role-separated DB credentials per process role.
2. Least privilege grants by schema.
3. API and worker DB URLs must both be present and must differ.
4. Migration/startup policy fail-closed on unsafe states.

## What This Prevents

1. API path reading secrets in `vault` when only worker/callback should access them.
2. Accidental privilege escalation from shared credentials.
3. Startup against partial/failed migration states.

## Required Configuration

1. `DATABASE_URL_API=postgres://api_runtime:...`
2. `DATABASE_URL_WORKER=postgres://worker_runtime:...`
3. `DATABASE_URL_MIGRATOR=postgres://db_migrator:...`
4. TLS required in non-local environments (`sslmode=require` policy).

## Privilege Baseline

1. `api_runtime`: runtime DML only; no DDL.
2. `worker_runtime`: worker DML scope; no DDL.
3. `db_migrator`: migration control-plane role with DDL authority.
4. `ops_readonly_human` and `ops_readonly_agent`: `ops_ro` views only.

## Software Setup Steps

1. Create DB roles and grants per environment.
2. Set role-specific URLs in runtime secret manager.
3. Verify role URLs are distinct.
4. Run migrations and ensure migration guard checks pass before serving.
5. Run `zombied doctor` for role/env posture confirmation.

## Verification

1. As `api_runtime`, `CREATE TABLE ...` must fail.
2. As `worker_runtime`, required runtime writes pass but DDL fails.
3. As readonly roles, direct `SELECT` from `vault.secrets` must fail.
4. Serve startup must fail when migration state is partial/failed/unsafe.
