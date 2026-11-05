-- A fleet's place in the lease queue, kept on its slot by the admission ledger.
--
-- `ready_at` is a NULL marker and an order key: the `created_at` of the fleet's
-- oldest unsettled admission while that admission is unparked, NULL otherwise.
-- It restates the ledger's head (rule 5 exception), kept exact by the trigger
-- below under the slot's row lock. Reader: the lease claim
-- (`afd_fleet` `claim_admission_head.sql`), which drives from the partial index
-- and takes the oldest ready, free slot. Readiness and occupancy stay apart:
-- a lease taking or lapsing the slot leaves `ready_at` alone, so a lapsed lease
-- is claimable at its deadline with no writer.
ALTER TABLE runner.fleet_slots ADD COLUMN ready_at TIMESTAMPTZ(3);

CREATE INDEX idx_fleet_slots_lease_expires_at_ready_at
    ON runner.fleet_slots (lease_expires_at, ready_at) WHERE ready_at IS NOT NULL;

-- Recomputed, never incremented, so every writer of `fleet.admissions` keeps it
-- exact. The slot is locked in a statement of its own before the head is read:
-- under READ COMMITTED each statement here takes a fresh snapshot, so a writer
-- that committed while this one waited on the slot is counted. Every update of
-- an existing admission already holds the slot (the lock order in
-- `docs/architecture/concurrency.md`); an admission insert waits here, holding
-- only a row no other transaction can see. A slot the trigger creates starts at
-- fence 0 with its deadline passed, so the first claim carries fence 1.
CREATE FUNCTION fleet.admissions_mark_ready() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet, runner
AS $$
DECLARE
    head_ready TIMESTAMPTZ(3);
BEGIN
    INSERT INTO runner.fleet_slots AS s
        (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
         ready_at, created_at, updated_at)
    VALUES (NEW.fleet_id, NULL, 0, clock_timestamp() - interval '1 millisecond', NULL,
            NULL, now(), now())
    ON CONFLICT (fleet_id) DO NOTHING;
    PERFORM 1 FROM runner.fleet_slots s WHERE s.fleet_id = NEW.fleet_id FOR UPDATE;
    SELECT CASE WHEN a.parked_at IS NULL THEN a.created_at END INTO head_ready
      FROM fleet.admissions a
     WHERE a.fleet_id = NEW.fleet_id AND a.settled_at IS NULL
     ORDER BY a.sequence
     LIMIT 1;
    UPDATE runner.fleet_slots s
       SET ready_at = head_ready, updated_at = now()
     WHERE s.fleet_id = NEW.fleet_id AND s.ready_at IS DISTINCT FROM head_ready;
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_admissions_mark_ready
    AFTER INSERT OR UPDATE OF settled_at, parked_at ON fleet.admissions
    FOR EACH ROW
    EXECUTE FUNCTION fleet.admissions_mark_ready();

-- Backfill: a slot for every fleet with unsettled work, then every slot's
-- readiness by the trigger's rule.
INSERT INTO runner.fleet_slots
    (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
     ready_at, created_at, updated_at)
SELECT DISTINCT a.fleet_id, NULL::uuid, 0, clock_timestamp() - interval '1 millisecond',
       NULL::timestamptz, NULL::timestamptz, now(), now()
  FROM fleet.admissions a
 WHERE a.settled_at IS NULL
ON CONFLICT (fleet_id) DO NOTHING;

UPDATE runner.fleet_slots s
   SET ready_at = head.ready_at, updated_at = now()
  FROM (SELECT DISTINCT ON (a.fleet_id) a.fleet_id,
               CASE WHEN a.parked_at IS NULL THEN a.created_at END AS ready_at
          FROM fleet.admissions a
         WHERE a.settled_at IS NULL
         ORDER BY a.fleet_id, a.sequence) head
 WHERE head.fleet_id = s.fleet_id AND head.ready_at IS NOT NULL;
