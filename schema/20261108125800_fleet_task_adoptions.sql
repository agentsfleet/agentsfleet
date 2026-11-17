-- A person's retry key for adopting a task. Ids are opaque so the key survives task deletion;
-- no goal text, authority or private evidence is copied.
CREATE TABLE fleet.task_adoptions (
    operation_id UUID NOT NULL,
    task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    request_digest TEXT NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    PRIMARY KEY (task_id, operation_id)
);

CREATE FUNCTION fleet.task_adoptions_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'task_adoptions is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_task_adoptions_refuse_update
    BEFORE UPDATE ON fleet.task_adoptions
    FOR EACH ROW EXECUTE FUNCTION fleet.task_adoptions_refuse_update();

GRANT SELECT, INSERT ON fleet.task_adoptions TO api_runtime;
