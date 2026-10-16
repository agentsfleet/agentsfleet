# Failed Migration

**Owners:** 🤠 Indy approves any data change the blocker needs; 🦉 Orly reads
the failure, fixes the blocker and redeploys.
**Scope:** one environment per run.

A deploy runs `agentsfleetd migrate` as Fly's `release_command`
(`deploy/fly/agentsfleetd-{dev,prod}/fly.toml`) before any machine is
replaced. Each slot applies in its own transaction, so a slot that fails
rolls back whole: the schema, the data and the applied ledger stay exactly
as the previous release left them, and the old machines keep serving. The
proof is `test_failed_cutover_release_keeps_data_refuses_image_rollback_and_rolls_forward`
in `rustd/crates/afd_db/tests/integration_migrate/cutover.rs`.

## What not to do

**Do not roll the image back to "retry".** The failed slot leaves a row in
`audit.schema_migration_failures`, and an older image's migrate refuses a
ledger that names a version it does not know (`UZ-STARTUP-005`). The old
machines are already serving; there is nothing to roll back to.

## Procedure

1. **Stop writes that would race the fix.** If the blocker is data (a row a
   new constraint refuses), pause the producers that write that table first.
   Schema-only blockers need no pause.
2. **Read the failure.** The deploy log's release step names the slot and the
   database's own sentence. The same text is in the ledger:

   ```sql
   SELECT version, failed_at, error_text
     FROM audit.schema_migration_failures ORDER BY failed_at DESC;
   ```

3. **Fix the blocker, not the migration.** A shipped slot file is frozen. The
   usual blockers are an object an operator created (a view or index on a
   column the slot drops) or a row the slot's new constraint refuses. Removing
   an operator's object is 🦉's call; changing or deleting tenant data is
   🤠's.
4. **Roll forward.** Redeploy the SAME image. Its release step reruns the
   failed slot; on success the failure row is cleared and the rolling deploy
   proceeds.
5. **Confirm.** The release step logs the applied versions, and the ledger
   query in step 2 returns no row for that version.
