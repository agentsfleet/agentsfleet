CREATE TABLE fleet.tasks (
    id                      UUID PRIMARY KEY,
    CONSTRAINT ck_tasks_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id                UUID NOT NULL,
    workspace_id            UUID NOT NULL,
    tenant_id               UUID NOT NULL,
    originating_task_item_id UUID NOT NULL,
    accepted_revision_id    UUID NOT NULL,
    title                   TEXT NOT NULL,
    objective               TEXT NOT NULL,
    questions               JSONB NOT NULL,
    goal_digest             TEXT NOT NULL,
    target                  JSONB NOT NULL,
    status                  TEXT NOT NULL,
    resource_version        BIGINT NOT NULL CHECK (resource_version > 0),
    generation              BIGINT NOT NULL CHECK (generation > 0),
    stop_generation         BIGINT NOT NULL CHECK (stop_generation >= 0),
    input_sequence          BIGINT NOT NULL CHECK (input_sequence >= 0),
    covered_input_sequence  BIGINT NOT NULL CHECK (covered_input_sequence >= 0),
    effect_cursor           BIGINT NOT NULL CHECK (effect_cursor >= 0),
    confirmed_effect_watermark BIGINT NOT NULL CHECK (confirmed_effect_watermark >= 0),
    max_model_nanos         BIGINT NOT NULL CHECK (max_model_nanos >= 0),
    used_model_nanos        BIGINT NOT NULL CHECK (used_model_nanos >= 0),
    reserved_model_nanos    BIGINT NOT NULL CHECK (reserved_model_nanos >= 0),
    max_running_nanos       BIGINT NOT NULL CHECK (max_running_nanos >= 0),
    used_running_nanos      BIGINT NOT NULL CHECK (used_running_nanos >= 0),
    max_leases              INTEGER NOT NULL CHECK (max_leases > 0),
    used_leases             INTEGER NOT NULL CHECK (used_leases >= 0),
    max_publications        INTEGER NOT NULL CHECK (max_publications > 0),
    used_publications       INTEGER NOT NULL CHECK (used_publications >= 0),
    max_decision_calls      INTEGER NOT NULL CHECK (max_decision_calls > 0),
    used_decision_calls     INTEGER NOT NULL CHECK (used_decision_calls >= 0),
    max_fallback_reads      INTEGER NOT NULL CHECK (max_fallback_reads > 0),
    used_fallback_reads     INTEGER NOT NULL CHECK (used_fallback_reads >= 0),
    reply_provider          TEXT,
    reply_address           TEXT,
    expires_at              TIMESTAMPTZ(3) NOT NULL,
    terminal_at             TIMESTAMPTZ(3),
    created_at              TIMESTAMPTZ(3) NOT NULL,
    updated_at              TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_tasks_fleet_scope FOREIGN KEY (fleet_id, workspace_id, tenant_id)
        REFERENCES fleet.fleets (id, workspace_id, tenant_id) ON DELETE CASCADE,
    CONSTRAINT fk_tasks_item FOREIGN KEY (originating_task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.task_items (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_tasks_revision FOREIGN KEY (accepted_revision_id, fleet_id)
        REFERENCES fleet.revisions (id, fleet_id),
    CONSTRAINT uq_tasks_originating_item UNIQUE (originating_task_item_id),
    CONSTRAINT uq_tasks_id_scope UNIQUE (id, fleet_id, workspace_id),
    CONSTRAINT ck_tasks_reply_both_or_neither
        CHECK ((reply_provider IS NULL) = (reply_address IS NULL)),
    CONSTRAINT ck_tasks_input_coverage CHECK (covered_input_sequence <= input_sequence),
    CONSTRAINT ck_tasks_effect_coverage CHECK (confirmed_effect_watermark <= effect_cursor),
    -- Close the task/checkpoint cycle after both owners exist.
    task_checkpoint_id UUID,
    -- NULL means no plan allocated; native allocation increments this monotonic cursor.
    checkpoint_sequence BIGINT CHECK (checkpoint_sequence >= 0),
    -- checkpoint/maintenance reads this indexed due marker; no queue/dispatcher is added.
    checkpoint_cleanup_at TIMESTAMPTZ(3),
    -- Monotonic allocation survives deletion of old private waits.
    wait_sequence BIGINT CHECK (wait_sequence >= 0),
    -- Bounded native reasons explain waits and blockers without retaining model text.
    status_reason TEXT,
    last_transition_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_tasks_fleet_unfinished ON fleet.tasks (fleet_id, id)
    WHERE terminal_at IS NULL;

CREATE INDEX idx_tasks_fleet_created_at_id ON fleet.tasks (fleet_id, created_at DESC, id DESC);

CREATE INDEX idx_tasks_expires_at_unfinished ON fleet.tasks (expires_at, id)
    WHERE terminal_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.tasks TO api_runtime;

-- Input association and the current-work list must not scan terminal history
-- to discover at most sixteen unfinished objectives in creation order.
CREATE INDEX idx_tasks_unfinished_created
    ON fleet.tasks (fleet_id, created_at DESC, id DESC)
    WHERE terminal_at IS NULL;

CREATE INDEX idx_task_checkpoint ON fleet.tasks (task_checkpoint_id) WHERE task_checkpoint_id IS NOT NULL;

CREATE INDEX idx_task_checkpoint_cleanup ON fleet.tasks (checkpoint_cleanup_at, id)
    WHERE checkpoint_cleanup_at IS NOT NULL;

-- A partial containment index uses the native target itself as its sole source.
CREATE INDEX idx_tasks_dependency_target ON fleet.tasks USING gin (target jsonb_path_ops)
    WHERE terminal_at IS NULL;

CREATE INDEX idx_tasks_unfinished_attempt_limit ON fleet.tasks (id)
    WHERE terminal_at IS NULL AND used_leases >= max_leases;

-- The association is assigned in the same binding transaction as the owner.
ALTER TABLE fleet.task_items
    ADD CONSTRAINT fk_task_items_task_scope FOREIGN KEY (task_id, fleet_id, workspace_id)
    REFERENCES fleet.tasks (id, fleet_id, workspace_id)
    DEFERRABLE INITIALLY DEFERRED;
