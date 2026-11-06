-- Only opaque storage locators survive owner deletion until remote cleanup lands.
CREATE TABLE fleet.task_storage_purges (
    task_object_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    task_identity_key_id UUID NOT NULL,
    bucket_reference TEXT NOT NULL,
    accounted_bytes BIGINT NOT NULL CHECK (accounted_bytes >= 0),
    stored BOOLEAN NOT NULL,
    delete_after_at TIMESTAMPTZ(3) NOT NULL,
    next_attempt_at TIMESTAMPTZ(3) NOT NULL,
    completed_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);
CREATE INDEX idx_task_storage_purges_due ON fleet.task_storage_purges (next_attempt_at, task_object_id)
    WHERE completed_at IS NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_storage_purges TO api_runtime;
