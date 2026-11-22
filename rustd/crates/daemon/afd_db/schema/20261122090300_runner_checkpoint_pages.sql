-- One bounded manifest page of a checkpoint or of a retained repository base, exactly one of the
-- two. Page identities are allocated with their owner; a page's first complete body is final.
CREATE TABLE runner.checkpoint_pages (
    id UUID PRIMARY KEY,
    checkpoint_id UUID REFERENCES runner.checkpoints (id) ON DELETE CASCADE,
    checkpoint_base_id UUID REFERENCES runner.checkpoint_bases (id) ON DELETE CASCADE,
    page_number INTEGER NOT NULL CHECK (page_number >= 0),
    digest TEXT,
    body BYTEA,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_checkpoint_pages_checkpoint_id_page_number UNIQUE (checkpoint_id, page_number),
    CONSTRAINT uq_checkpoint_pages_checkpoint_base_id_page_number UNIQUE (checkpoint_base_id, page_number),
    CONSTRAINT ck_checkpoint_pages_one_owner CHECK (num_nonnulls(checkpoint_id, checkpoint_base_id) = 1),
    CONSTRAINT ck_checkpoint_pages_body CHECK ((digest IS NULL) = (body IS NULL))
);

CREATE FUNCTION runner.checkpoint_pages_freeze() RETURNS trigger AS $$
BEGIN
    IF OLD.body IS NOT NULL OR
       ROW(NEW.id, NEW.checkpoint_id, NEW.checkpoint_base_id, NEW.page_number) IS DISTINCT FROM
       ROW(OLD.id, OLD.checkpoint_id, OLD.checkpoint_base_id, OLD.page_number) THEN
        RAISE EXCEPTION 'completed checkpoint pages are immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_checkpoint_pages_freeze BEFORE UPDATE ON runner.checkpoint_pages
    FOR EACH ROW EXECUTE FUNCTION runner.checkpoint_pages_freeze();

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.checkpoint_pages TO api_runtime;
