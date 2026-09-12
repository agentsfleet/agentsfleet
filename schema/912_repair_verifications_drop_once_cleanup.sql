-- The Redis once-key cleanup column, its index, and its trigger arm, removed.
--
-- `redis_once_key_cleared_at` recorded when a dispatched verification's
-- at-most-once marker key had been expired from Redis. That marker no longer
-- exists: acceptance became a `core.fleet_admissions` row, the two-key
-- `append_once` script was deleted with its `OnceScope`, and the sweeper that
-- cleared the keys (`afd_runner::sweep::repair::cleanup`) went with it. Nothing
-- has written this column since.
--
-- Leaving it was not free. `idx_repair_verifications_redis_cleanup` is a PARTIAL
-- index whose predicate is `verifier_event_id IS NOT NULL AND
-- redis_once_key_cleared_at IS NULL` — it used to mean "dispatched, key not yet
-- cleared", a set that drained. With nothing writing the column the predicate
-- is true of EVERY dispatched row for the rest of the deployment's life, so the
-- index grows without bound, is written on every completion, and is read by
-- nothing.
--
-- The trigger's third arm went the same way. It permitted exactly the NULL ->
-- NOT NULL transition on this column; with no writer it authorised a statement
-- nobody issues, which is a rule guarding nothing (RULE NLR).
--
-- Forward migration, not an edit to schema/835: `VERSION` is at the 0.30.0
-- anchor, so shipped slot files are frozen history. The function is replaced
-- BEFORE the column goes, because a plpgsql body naming a dropped column fails
-- at call time rather than at replace time — the ordering is what keeps a
-- verification dispatched between the two statements from raising.

CREATE OR REPLACE FUNCTION core.repair_verifications_fenced_update() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' AND current_setting('fleet.allow_gate_purge', true) = 'on' THEN
        RETURN OLD;
    END IF;
    IF TG_OP = 'UPDATE'
        AND NEW.id IS NOT DISTINCT FROM OLD.id
        AND NEW.workspace_id IS NOT DISTINCT FROM OLD.workspace_id
        AND NEW.production_result_id IS NOT DISTINCT FROM OLD.production_result_id
        AND NEW.repair_link_id IS NOT DISTINCT FROM OLD.repair_link_id
        AND NEW.verifier_fleet_id IS NOT DISTINCT FROM OLD.verifier_fleet_id
        AND NEW.verify_after IS NOT DISTINCT FROM OLD.verify_after
        AND NEW.created_at IS NOT DISTINCT FROM OLD.created_at
        AND (
            (OLD.verifier_event_id IS NULL
                AND NEW.verifier_event_id IS NULL
                AND NEW.dispatch_claim_token IS NOT NULL
                AND NEW.dispatch_claimed_at IS NOT NULL
                AND NEW.dispatch_attempts = OLD.dispatch_attempts + 1
                AND NEW.updated_at = NEW.dispatch_claimed_at
            )
            OR (OLD.verifier_event_id IS NULL
                AND NEW.verifier_event_id IS NOT NULL
                AND OLD.dispatch_claim_token IS NOT NULL
                AND NEW.dispatch_claim_token IS NULL
                AND NEW.dispatch_claimed_at IS NULL
                AND NEW.dispatch_attempts = OLD.dispatch_attempts
                AND NEW.updated_at >= OLD.updated_at)
        )
    THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'repair_verifications permits a fenced claim, then event completion';
END;
$$ LANGUAGE plpgsql;

DROP INDEX IF EXISTS core.idx_repair_verifications_redis_cleanup;

ALTER TABLE core.repair_verifications DROP COLUMN IF EXISTS redis_once_key_cleared_at;
