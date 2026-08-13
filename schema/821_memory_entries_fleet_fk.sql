-- Additive migration: a memory row gains the referential edge to its fleet.
--
-- Why: `memory.memory_entries.fleet_id` carried the owning fleet's UUID as a
-- bare value with no foreign key, keeping the memory role boundary free of a
-- `core` dependency. The cost was that nothing except an explicit DELETE could
-- erase a memory row. An account purge (src/agentsfleetd/state/account_teardown.zig)
-- sweeps memory from a FROZEN id array, so a capture landing after that
-- statement outlived the erasure — no cascade behind it, and no later pass that
-- would ever find it.
--
-- The edge closes that hole. A capture racing a purge now either commits before
-- the fleet row goes, and the cascade erases it, or it blocks on that row's lock
-- and fails closed once the parent is gone. Erasure becomes exact instead of a
-- race. Adding the constraint also validates every existing row, so this
-- migration applying cleanly is itself the proof that no orphan is present.
--
-- Runtime privilege separation is unchanged: `memory_runtime` gains no `core`
-- grant. REFERENCES is a DDL-time privilege held by the migrator, and PostgreSQL
-- evaluates both the check and the cascade with the table owner's authority —
-- the same mechanism schema/700 and schema/710 already rely on to erase the
-- wallet and the ledger without a billing elevation.
--
-- Idempotent via DROP-then-ADD: PostgreSQL has no ADD CONSTRAINT IF NOT EXISTS,
-- and dropping a constraint that was never created is a no-op under IF EXISTS.

ALTER TABLE memory.memory_entries
    DROP CONSTRAINT IF EXISTS fk_memory_entries_fleet_id;

ALTER TABLE memory.memory_entries
    ADD CONSTRAINT fk_memory_entries_fleet_id
    FOREIGN KEY (fleet_id) REFERENCES core.fleets (id) ON DELETE CASCADE;
