# Redis Security

## Why This Exists

Redis is the queue coordination plane. If misconfigured, an API identity could steal worker messages or a worker identity could write unauthorized keys.

## Decisions

1. Separate credentials for API and worker.
2. TLS required in hardened environments (`rediss://`).
3. API/worker URLs must be explicitly set and must differ.
4. Readiness and doctor check queue operability, not only socket reachability.

## What This Prevents

1. Message theft by API role (`XREADGROUP` denial by ACL policy).
2. Arbitrary writes by worker role outside queue scope.
3. Silent plaintext transport downgrade.
4. Startup in unsafe role-collapsed configuration.

## Required Configuration

1. `REDIS_URL_API=rediss://api_user:<pass>@<host>:6379`
2. `REDIS_URL_WORKER=rediss://worker_user:<pass>@<host>:6379`
3. Optional local self-signed CA path: `REDIS_TLS_CA_CERT_FILE=/absolute/path/to/ca.crt`

## ACL Baseline

```text
ACL SETUSER api_user on >api_password ~run_queue +xadd +xgroup +ping
ACL SETUSER worker_user on >worker_password ~run_queue +xreadgroup +xack +xautoclaim +xgroup +ping +xinfo
ACL SETUSER default off
```

## Software Setup Steps

1. Provision Redis with ACL support (Redis 7+).
2. Create role users and apply ACL policy.
3. Configure API and worker with different role URLs.
4. Ensure queue stream/group exists (`run_queue`, `workers`) via startup guard or migration bootstrap.
5. Run `zombied doctor` and confirm Redis readiness + ACL identity checks pass.

## Verification

1. `zombied doctor` should report Redis API/worker readiness checks as OK.
2. `/readyz` should fail closed when queue dependency is degraded.
3. Integration tests should include TLS and group operability checks where env is configured.
