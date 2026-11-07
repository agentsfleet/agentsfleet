-- A lost database result must outlive the normal server statement cancellation.
-- The advisory lock is held and released by this scenario's owned transaction.
CREATE FUNCTION fleet.stall_task_read() RETURNS boolean AS $$
BEGIN
    -- A stuck server ignores the API login's lock bound too (afd_db::login).
    PERFORM set_config('lock_timeout', '0', true);
    PERFORM pg_advisory_xact_lock(872365);
    RETURN true;
EXCEPTION WHEN query_canceled THEN
    PERFORM pg_advisory_xact_lock(872365);
    RETURN true;
END;
$$ LANGUAGE plpgsql;
ALTER TABLE fleet.tasks ENABLE ROW LEVEL SECURITY;
CREATE POLICY stalled_read ON fleet.tasks USING (fleet.stall_task_read());
