-- Base metadata survives deletion of the task that first captured it.
CREATE TABLE fleet.durable_task_base_pages (
    id UUID PRIMARY KEY,
    durable_task_base_id UUID NOT NULL REFERENCES fleet.durable_task_bases (id) ON DELETE CASCADE,
    page_number INTEGER NOT NULL CHECK (page_number >= 0),
    digest TEXT,
    body BYTEA,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_durable_task_base_page_number UNIQUE (durable_task_base_id, page_number),
    CONSTRAINT ck_durable_task_base_page_body CHECK ((digest IS NULL) = (body IS NULL))
);

CREATE FUNCTION fleet.durable_task_base_pages_freeze() RETURNS trigger AS $$
BEGIN
    IF OLD.body IS NOT NULL OR
       ROW(NEW.id, NEW.durable_task_base_id, NEW.page_number) IS DISTINCT FROM
       ROW(OLD.id, OLD.durable_task_base_id, OLD.page_number) THEN
        RAISE EXCEPTION 'completed base pages are immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_base_pages_freeze BEFORE UPDATE ON fleet.durable_task_base_pages
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_base_pages_freeze();

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.durable_task_base_pages TO api_runtime;
