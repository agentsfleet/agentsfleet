-- Page identities are allocated first; acknowledged bytes never change.
CREATE TABLE runner.turn_checkpoint_pages (
    id UUID PRIMARY KEY,
    turn_checkpoint_id UUID NOT NULL REFERENCES runner.turn_checkpoints (id) ON DELETE CASCADE,
    page_number INTEGER NOT NULL CHECK (page_number >= 0),
    digest TEXT,
    body BYTEA,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_turn_checkpoint_page UNIQUE (turn_checkpoint_id, page_number),
    CONSTRAINT ck_turn_page_body CHECK ((digest IS NULL) = (body IS NULL))
);

CREATE FUNCTION runner.turn_checkpoint_pages_freeze() RETURNS trigger AS $$
BEGIN
    IF OLD.body IS NOT NULL OR ROW(NEW.id, NEW.turn_checkpoint_id, NEW.page_number)
       IS DISTINCT FROM ROW(OLD.id, OLD.turn_checkpoint_id, OLD.page_number) THEN
        RAISE EXCEPTION 'completed turn_checkpoint_pages is append-only';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_turn_checkpoint_pages_freeze BEFORE UPDATE ON runner.turn_checkpoint_pages
    FOR EACH ROW EXECUTE FUNCTION runner.turn_checkpoint_pages_freeze();

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.turn_checkpoint_pages TO api_runtime;
