-- Publication authority, recorded before any branch mutation, and its acknowledgement. The
-- request columns are written once; only `status`, `commit_oid` and the private `target` change.
-- Ids are opaque, with no foreign key, so the effect and its receipt survive task and fleet
-- purge; a task's purge clears only its private `target`.
CREATE TABLE fleet.durable_task_effects (
    id UUID PRIMARY KEY,
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    source_item_id UUID NOT NULL,
    original_run_id UUID NOT NULL,
    original_runner_id UUID NOT NULL,
    original_fence BIGINT NOT NULL,
    durable_task_generation BIGINT NOT NULL CHECK (durable_task_generation > 0),
    stop_generation BIGINT NOT NULL CHECK (stop_generation >= 0),
    ordinal BIGINT NOT NULL CHECK (ordinal > 0),
    operation_id UUID NOT NULL,
    covered_input_sequence BIGINT NOT NULL CHECK (covered_input_sequence >= 0),
    expected_head TEXT NOT NULL,
    tested_tree TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    github_repository_id BIGINT NOT NULL,
    github_installation_id BIGINT NOT NULL,
    issuance_expires_at TIMESTAMPTZ(3) NOT NULL,
    -- The acknowledgement (`prepared` through `confirmed`, `unknown` or `refused`) and the
    -- confirmed remote commit, read by `afd_fleet/src/durable_task/effects/sql/`.
    status TEXT NOT NULL,
    commit_oid TEXT,
    -- Private repository, branch and workflow details; NULL once the task is purged.
    target JSONB,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_durable_task_effects_ordinal UNIQUE (durable_task_id, ordinal),
    CONSTRAINT uq_durable_task_effects_operation UNIQUE (durable_task_id, operation_id)
);

-- The request never changes, and a target is only ever cleared.
CREATE FUNCTION fleet.durable_task_effects_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_effects request is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_effects_refuse_update BEFORE UPDATE OF id, durable_task_id,
    fleet_id, workspace_id, source_item_id, original_run_id, original_runner_id, original_fence,
    durable_task_generation, stop_generation, ordinal, operation_id, covered_input_sequence,
    expected_head, tested_tree, request_digest, github_repository_id, github_installation_id,
    issuance_expires_at, created_at ON fleet.durable_task_effects
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_effects_refuse_update();

CREATE TRIGGER trg_durable_task_effects_refuse_target BEFORE UPDATE OF target
    ON fleet.durable_task_effects
    FOR EACH ROW WHEN (NEW.target IS NOT NULL)
    EXECUTE FUNCTION fleet.durable_task_effects_refuse_update();

-- A task's purge clears its effects' private target in the same statement; the opaque effect
-- and its acknowledgement stay. SECURITY DEFINER, as the payload's cascade ran as the owner, so
-- the purge never depends on the deleting role's grants.
CREATE FUNCTION fleet.durable_task_effects_purge_target() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet
AS $$
BEGIN
    UPDATE fleet.durable_task_effects SET target = NULL, updated_at = now()
     WHERE durable_task_id = OLD.id AND fleet_id = OLD.fleet_id
       AND workspace_id = OLD.workspace_id AND target IS NOT NULL;
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_durable_tasks_purge_effect_target AFTER DELETE ON fleet.durable_tasks
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_effects_purge_target();

GRANT SELECT, INSERT ON fleet.durable_task_effects TO api_runtime;
GRANT UPDATE (status, commit_oid, target, updated_at) ON fleet.durable_task_effects TO api_runtime;
