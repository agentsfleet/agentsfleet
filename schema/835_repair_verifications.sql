-- One bounded verifier dispatch intent per production result, repair, and Fleet.

CREATE TABLE IF NOT EXISTS core.repair_verifications (
    id                   UUID   PRIMARY KEY,
    CONSTRAINT ck_repair_verifications_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id         UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    production_result_id UUID   NOT NULL REFERENCES core.repair_production_results(id) ON DELETE CASCADE,
    repair_link_id       UUID   NOT NULL REFERENCES core.repair_pr_links(id) ON DELETE CASCADE,
    verifier_fleet_id    UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    verify_after         BIGINT NOT NULL,
    verifier_event_id    TEXT,
    dispatch_claim_token UUID,
    dispatch_claimed_at  BIGINT,
    dispatch_attempts    BIGINT NOT NULL,
    redis_once_key_cleared_at BIGINT,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    CONSTRAINT uq_repair_verifications_attempt
        UNIQUE (production_result_id, repair_link_id, verifier_fleet_id)
);

CREATE INDEX IF NOT EXISTS idx_repair_verifications_due
    ON core.repair_verifications (verify_after, id)
    WHERE verifier_event_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_repair_verifications_workspace
    ON core.repair_verifications (workspace_id);

CREATE INDEX IF NOT EXISTS idx_repair_verifications_repair_link
    ON core.repair_verifications (repair_link_id);

CREATE INDEX IF NOT EXISTS idx_repair_verifications_verifier_fleet
    ON core.repair_verifications (verifier_fleet_id);

CREATE INDEX IF NOT EXISTS idx_fleets_workspace_status_id
    ON core.fleets (workspace_id, status, id);

CREATE INDEX IF NOT EXISTS idx_repair_verifications_redis_cleanup
    ON core.repair_verifications (updated_at, id)
    WHERE verifier_event_id IS NOT NULL AND redis_once_key_cleared_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_repair_verifications_verifier_event
    ON core.repair_verifications (verifier_fleet_id, verifier_event_id)
    WHERE verifier_event_id IS NOT NULL;

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
                AND OLD.redis_once_key_cleared_at IS NULL
                AND NEW.redis_once_key_cleared_at IS NULL
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
                AND NEW.redis_once_key_cleared_at IS NULL
                AND NEW.updated_at >= OLD.updated_at)
            OR (OLD.verifier_event_id IS NOT NULL
                AND NEW.verifier_event_id IS NOT DISTINCT FROM OLD.verifier_event_id
                AND OLD.dispatch_claim_token IS NULL
                AND NEW.dispatch_claim_token IS NULL
                AND NEW.dispatch_claimed_at IS NULL
                AND NEW.dispatch_attempts = OLD.dispatch_attempts
                AND OLD.redis_once_key_cleared_at IS NULL
                AND NEW.redis_once_key_cleared_at IS NOT NULL
                AND NEW.updated_at >= OLD.updated_at)
        )
    THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'repair_verifications permits fenced claim, event completion, then Redis key cleanup';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_repair_verifications_fenced_update ON core.repair_verifications;
CREATE TRIGGER trg_repair_verifications_fenced_update
    BEFORE UPDATE OR DELETE ON core.repair_verifications
    FOR EACH ROW EXECUTE FUNCTION core.repair_verifications_fenced_update();

GRANT SELECT, INSERT, UPDATE ON core.repair_verifications TO api_runtime;
