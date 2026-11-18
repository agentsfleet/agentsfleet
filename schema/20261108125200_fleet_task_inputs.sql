-- Accepted input meaning, kept apart from pending queue delivery. `sequence` is per-task order,
-- allocated under the task row's lock. NULL `answered_at` means not yet answered.
CREATE TABLE fleet.task_inputs (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_task_inputs_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    task_id             UUID NOT NULL,
    fleet_id            UUID NOT NULL,
    workspace_id        UUID NOT NULL,
    task_item_id        UUID NOT NULL,
    sequence            BIGINT NOT NULL CHECK (sequence > 0),
    accepted_text       TEXT NOT NULL,
    interpretation      TEXT NOT NULL,
    generation          BIGINT NOT NULL CHECK (generation > 0),
    answered_at         TIMESTAMPTZ(3),
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_task_inputs_task_scope FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_task_inputs_item_scope FOREIGN KEY (task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.task_items (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT uq_task_inputs_task_item UNIQUE (task_id, task_item_id),
    CONSTRAINT uq_task_inputs_task_sequence UNIQUE (task_id, sequence)
);

CREATE INDEX idx_task_inputs_unanswered ON fleet.task_inputs (task_id, sequence)
    WHERE answered_at IS NULL;

CREATE INDEX idx_task_inputs_item_scope ON fleet.task_inputs (task_item_id, fleet_id, workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_inputs TO api_runtime;
