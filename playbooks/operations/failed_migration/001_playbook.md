---
type: troubleshooting
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Recover a failed migration

## Symptom

A deploy's release step fails with `migration_failed`, naming the slot `version` and the database's own error.
The old machines keep serving the previous release.

## Why it happened

Fly runs `/usr/local/bin/agentsfleetd migrate` as `release_command` (`deploy/fly/agentsfleetd-{dev,prod}/fly.toml`) before replacing any machine.
Each slot applies in its own transaction, so a failed slot rolls back whole and is never recorded in `audit._sqlx_migrations`.
Usual blockers: an operator-created object on a column the slot drops, or a row the slot's new constraint refuses.

## How to fix it

Do not roll the image back; nothing is left to undo, and the next deploy meets the same blocker.
Work one environment per run.
🤠 Indy approves any tenant data change; 🦉 the agent removes operator-created objects, fixes the blocker and redeploys.

1. If the blocker is data, pause the producers that write that table.
   Schema-only blockers need no pause.
2. Read the ledger; it ends at the last applied slot.

   ```sql
   SELECT version, description, installed_on
     FROM audit._sqlx_migrations ORDER BY version DESC LIMIT 5;
   ```

   ```text
   <varies: the newest row is the slot before the failed version>
   ```

3. Fix the blocker, never the migration; a shipped slot file is frozen.
4. Redeploy the same image.
   Its release step reruns the failed slot, and the rolling deploy proceeds on success.
5. Rerun the step 2 query.
   It now returns a row for the failed version.
