# Reconciliation

## Overview

`zombied reconcile` processes stale outbox rows and recovers runs stuck in non-terminal states. It is designed to be run as a periodic cron job or scheduled task.

## What it does

The reconciler scans for:

- **Stale outbox rows** — Transactional outbox entries that were written to PostgreSQL but never picked up or acknowledged. These can occur when a worker crashes between writing the outbox row and completing the downstream action (webhook delivery, status update).
- **Stuck runs** — Runs that remain in a non-terminal state (`PLANNED`, `RUNNING`) beyond the expected timeout window. These can occur when a worker crashes mid-execution and no other worker reclaims the work.

For each stale item:

1. If the run's timeout has elapsed, mark it as `FAILED` with error code `UZ-EXEC-014` (lease expired).
2. If the outbox row has a pending webhook delivery, retry the delivery.
3. Record the reconciliation action in the run's event log.

## Usage

```bash
# Run reconciliation once
zombied reconcile

# Dry run — show what would be reconciled without making changes
zombied reconcile --dry-run
```

## Scheduling

Run the reconciler on a regular schedule. Every 5 minutes is a reasonable default:

```bash
# crontab entry
*/5 * * * * /opt/zombie/bin/zombied reconcile >> /var/log/zombie/reconcile.log 2>&1
```

Or as a systemd timer:

```ini
# zombied-reconcile.timer
[Unit]
Description=Run zombied reconciliation every 5 minutes

[Timer]
OnCalendar=*:0/5
Persistent=true

[Install]
WantedBy=timers.target
```

```ini
# zombied-reconcile.service
[Unit]
Description=zombied reconciliation

[Service]
Type=oneshot
EnvironmentFile=/opt/zombie/.env
ExecStart=/opt/zombie/bin/zombied reconcile
```

## Idempotency

The reconciler is idempotent. Running it multiple times on the same stale data produces the same result. It uses database-level locks to prevent concurrent reconciliation from conflicting.

## Monitoring

Watch the `reconcile_runs_total` and `reconcile_errors_total` metrics to track reconciliation activity. A sustained increase in reconciled runs may indicate worker instability.
