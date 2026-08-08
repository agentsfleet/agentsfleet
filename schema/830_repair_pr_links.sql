-- Incident → repair Pull Request → deploy-result linkage. One row per shipped
-- repair, written when the repairer's draft PR opens (the webhook arm matches
-- the agentsfleet-repair/<event id> branch prefix) and stamped when a
-- completed workflow run lands on that branch.
--
-- This is the deferred verifier member reduced to data: "did the fix work" is
-- a column an operator reads, not a model run. History layer (8xx), beside
-- the events and gates it links.
--
--   event_id       the incident event the repair answers; UNIQUE per fleet —
--                  the duplicate refusal at the data layer.
--   deploy_status  app-enforced vocabulary (RULE STS, no CHECK):
--                  pending | deploy_ok | deploy_failed.
--   Content columns are immutable; deploy_status + deploy_stamped_at are the
--   only mutable pair, held by trigger below. DELETE is refused outright.

CREATE TABLE IF NOT EXISTS core.repair_pr_links (
    id                UUID   PRIMARY KEY,
    CONSTRAINT ck_repair_pr_links_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id      UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    fleet_id          UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    event_id          TEXT   NOT NULL,
    repository        TEXT   NOT NULL,
    branch            TEXT   NOT NULL,
    pr_number         BIGINT NOT NULL,
    pr_url            TEXT   NOT NULL,
    deploy_status     TEXT   NOT NULL,
    deploy_stamped_at BIGINT,
    created_at        BIGINT NOT NULL,
    CONSTRAINT uq_repair_pr_links_fleet_event UNIQUE (fleet_id, event_id)
);

-- Reader: the deploy-stamp arm, keyed by the branch the workflow ran on.
CREATE INDEX IF NOT EXISTS idx_repair_pr_links_fleet_id_branch
    ON core.repair_pr_links (fleet_id, branch);

-- Immutability, held by the schema rather than by store discipline: content
-- columns never change after insert, and rows never leave — except by the
-- sanctioned hard-purge cascade, which opts in with the same
-- transaction-scoped setting the gates table honours (one purge switch for
-- the whole history layer; account erasure and fleet hard-delete already set
-- it). Column-equality comparisons only (no text pattern-matching on inputs).
CREATE OR REPLACE FUNCTION core.repair_pr_links_content_frozen() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF current_setting('fleet.allow_gate_purge', true) = 'on' THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'repair_pr_links is append-only -- DELETE is not permitted';
    END IF;
    IF NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
        OR NEW.fleet_id   IS DISTINCT FROM OLD.fleet_id
        OR NEW.event_id   IS DISTINCT FROM OLD.event_id
        OR NEW.repository IS DISTINCT FROM OLD.repository
        OR NEW.branch     IS DISTINCT FROM OLD.branch
        OR NEW.pr_number  IS DISTINCT FROM OLD.pr_number
        OR NEW.pr_url     IS DISTINCT FROM OLD.pr_url
        OR NEW.created_at IS DISTINCT FROM OLD.created_at
    THEN
        RAISE EXCEPTION 'repair_pr_links content is immutable -- only the deploy stamp may change';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_repair_pr_links_content_frozen ON core.repair_pr_links;
CREATE TRIGGER trg_repair_pr_links_content_frozen
    BEFORE UPDATE OR DELETE ON core.repair_pr_links
    FOR EACH ROW EXECUTE FUNCTION core.repair_pr_links_content_frozen();

-- No DELETE grant: removal is the purge cascade's, gated by the setting above.
GRANT SELECT, INSERT, UPDATE ON core.repair_pr_links TO api_runtime;
