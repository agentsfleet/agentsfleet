-- Count unique physical objects, including unfinished uploads and pending deletion.
CREATE TABLE fleet.task_storage_quota (
    workspace_id UUID PRIMARY KEY REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    reserved_bytes BIGINT NOT NULL CHECK (reserved_bytes >= 0),
    stored_bytes BIGINT NOT NULL CHECK (stored_bytes >= 0),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_storage_quota TO api_runtime;
