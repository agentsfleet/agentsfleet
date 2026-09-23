-- Where an owed answer is delivered: the address its producer recorded.
--
-- An obligation used to carry only a `provider`, and the report filled it with
-- the lease's MODEL provider, so every non-empty answer was owed to a connector
-- that does not exist. The report now owes a delivery only when the event it
-- settles was admitted with a destination (slot 918), and the obligation keeps
-- that destination's address beside its connector: the poster reads the thread
-- from the job it is handed rather than re-reading the event it came from.
--
-- Nullable, because every row written before this slot has none. Those rows
-- were owed to a model provider and can never be delivered; the recovery scans
-- stop offering them once they require a destination, and they leave with
-- their fleet by cascade — no data is rewritten here.
--
-- RULE SGR does not apply: no object is created, and access runs through the
-- table grants in schema/913_fleet_obligations.sql.
ALTER TABLE core.fleet_obligations
    ADD COLUMN IF NOT EXISTS destination TEXT;
