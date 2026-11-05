-- Public task progress survives reconnects separately from private recovery.
ALTER TABLE fleet.events ADD COLUMN task_activity JSONB NOT NULL
    DEFAULT '{"items":[],"truncated":false}'::jsonb;
ALTER TABLE fleet.events ADD CONSTRAINT ck_events_task_activity_bound
    CHECK (jsonb_typeof(task_activity->'items') = 'array'
        AND jsonb_array_length(task_activity->'items') <= 128
        AND octet_length(task_activity::text) <= 131072);
