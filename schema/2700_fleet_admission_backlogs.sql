-- Each fleet's count of unsettled admissions, read by the intake statement
-- (`afd_admission/src/sql/admit_one.sql`), which locks this row instead of the
-- fleet row while it checks the fleet budget and inserts.
--
-- Consistency rule (SQL-ONEFACT's trigger-kept aggregate): `unsettled_count`
-- equals the fleet's `fleet.admissions` rows with `settled_at IS NULL`. Two
-- triggers keep it, and the runtime role cannot write it. Keyed by fleet, one
-- row per fleet (SQL-ID's keyed-by-parent exception).
--
-- Not a `fleet.fleets` column: a report's commit would then wait on any
-- revision install holding the fleet row `FOR UPDATE`.

CREATE TABLE fleet.admission_backlogs (
    fleet_id         UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    unsettled_count  BIGINT NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ(3) NOT NULL,
    updated_at       TIMESTAMPTZ(3) NOT NULL
);

-- SECURITY DEFINER with a pinned search_path: api_runtime inserts admissions but
-- may not set the count, so only these functions move it.
CREATE FUNCTION fleet.admissions_backlog_add() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet
AS $$
BEGIN
    INSERT INTO fleet.admission_backlogs (fleet_id, unsettled_count, created_at, updated_at)
    VALUES (NEW.fleet_id, 1, now(), now())
    ON CONFLICT (fleet_id) DO UPDATE
       SET unsettled_count = fleet.admission_backlogs.unsettled_count + 1,
           updated_at = now();
    RETURN NULL;
END;
$$;

CREATE FUNCTION fleet.admissions_backlog_release() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet
AS $$
BEGIN
    UPDATE fleet.admission_backlogs
       SET unsettled_count = unsettled_count - 1, updated_at = now()
     WHERE fleet_id = OLD.fleet_id;
    RETURN NULL;
END;
$$;

-- An insert counts at once, inside the statement that admits it.
CREATE TRIGGER trg_admissions_backlog_add
    AFTER INSERT ON fleet.admissions
    FOR EACH ROW WHEN (NEW.settled_at IS NULL)
    EXECUTE FUNCTION fleet.admissions_backlog_add();

-- A first settle releases at commit. A plain AFTER trigger would hold this row
-- from a report's settle statement through its remaining writes, stalling the
-- fleet's intake behind every report. A re-stamp of a settled row releases nothing.
CREATE CONSTRAINT TRIGGER trg_admissions_backlog_release
    AFTER UPDATE OF settled_at ON fleet.admissions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW WHEN (OLD.settled_at IS NULL AND NEW.settled_at IS NOT NULL)
    EXECUTE FUNCTION fleet.admissions_backlog_release();

-- No backfill: the slot ships before production, and the deploy resets dev
-- data, so no unsettled admission predates the triggers (Indy, M242_001 Discovery).

GRANT SELECT ON fleet.admission_backlogs TO api_runtime;
GRANT INSERT (fleet_id, created_at, updated_at) ON fleet.admission_backlogs TO api_runtime;
GRANT UPDATE (updated_at) ON fleet.admission_backlogs TO api_runtime;
