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

1. `DATABASE_URL_API=postgres://api_accessor:...`
2. `DATABASE_URL_WORKER=postgres://worker_accessor:...`
3. (If used) `DATABASE_URL_CALLBACK=postgres://callback_accessor:...`
4. TLS required in non-local environments (`sslmode=require` policy).

## Privilege Baseline

1. `api_accessor`: no `vault` schema read/write.
2. `worker_accessor`: required `vault` access for BYOK runtime reads.
3. `callback_accessor`: minimal callback/write scope as designed.

## Software Setup Steps

1. Create DB roles and grants per environment.
2. Set role-specific URLs in runtime secret manager.
3. Verify role URLs are distinct.
4. Run migrations and ensure migration guard checks pass before serving.
5. Run `zombied doctor` for role/env posture confirmation.

## Verification

1. As `api_accessor`, `SELECT * FROM vault.secrets` must fail.
2. As `worker_accessor`, required vault operations must pass.
3. Serve startup must fail when migration state is partial/failed/unsafe.
