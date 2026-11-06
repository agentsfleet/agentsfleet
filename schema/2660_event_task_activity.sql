-- Public task progress survives reconnects separately from private recovery.
ALTER TABLE fleet.events ADD COLUMN task_activity JSONB NOT NULL
    DEFAULT jsonb_build_array();
ALTER TABLE fleet.events ADD COLUMN task_activity_sequence BIGINT NOT NULL
    DEFAULT 0 CHECK (task_activity_sequence >= 0);
ALTER TABLE fleet.events ADD COLUMN task_activity_truncated BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE fleet.events ADD CONSTRAINT ck_events_task_activity_bound
    -- jsonb_array_length refuses non-arrays. The write's 120 KB payload bound
    -- leaves space for metadata inside the public history's 128 KiB ceiling.
    CHECK (jsonb_array_length(task_activity) <= 128
        AND octet_length(task_activity::text) <= 120000);
