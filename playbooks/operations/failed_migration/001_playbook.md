# Failed Migration

**Owners:** 🤠 Indy approves any data change the blocker needs; 🦉 Orly reads
the failure, fixes the blocker and redeploys.
**Scope:** one environment per run.

A deploy runs `agentsfleetd migrate` as Fly's `release_command`
(`deploy/fly/agentsfleetd-{dev,prod}/fly.toml`) before any machine is
replaced. Each slot applies in its own transaction, so a slot that fails
rolls back whole: the schema, the data and the applied ledger
(`audit._sqlx_migrations`) stay exactly as the previous release left them, and
the old machines keep serving. The proofs are `test_a_failed_slot_is_never_recorded`
in `rustd/crates/afd_db/tests/integration_migrate/locking.rs` and
`a_runtime_batch_failure_rolls_back_and_a_corrected_retry_applies_once` in
`rustd/crates/afd_db/tests/integration_migrate_batch.rs`.

## What not to do

**Do not roll the image back to "retry".** The old machines are already
serving on the previous release, and the failed slot left nothing behind to
undo. Rolling back only delays the fix; the next deploy of the new image meets
the same blocker.

## Procedure

1. **Stop writes that would race the fix.** If the blocker is data (a row a
   new constraint refuses), pause the producers that write that table first.
   Schema-only blockers need no pause.
2. **Read the failure.** The deploy log's release step names the slot
   (`migration_failed`, with its `version`) and the database's own sentence.
   A failed slot is never recorded, so the ledger shows the last slot that
   applied:

   ```sql
   SELECT version, description, installed_on
     FROM audit._sqlx_migrations ORDER BY version DESC LIMIT 5;
   ```

3. **Fix the blocker, not the migration.** A shipped slot file is frozen. The
   usual blockers are an object an operator created (a view or index on a
   column the slot drops) or a row the slot's new constraint refuses. Removing
   an operator's object is 🦉's call; changing or deleting tenant data is
   🤠's.
4. **Roll forward.** Redeploy the SAME image. Its release step reruns the
   failed slot; on success the rolling deploy proceeds.
5. **Confirm.** The release step logs the applied versions, and the ledger
   query in step 2 now returns a row for that version.
