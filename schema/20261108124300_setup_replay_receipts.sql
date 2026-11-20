-- Every setup retry key. The same key and body within 24 hours of `created_at` return the
-- first result; after that the key is new and the row is swept. Append-only, and no foreign keys:
-- a key must outlive erasure of what it created, or a late retry would create it again.
CREATE TABLE setup.replay_receipts (
    id              UUID PRIMARY KEY,
    workspace_id    UUID   NOT NULL,
    user_id         UUID   NOT NULL,
    scope           TEXT   NOT NULL,
    operation_id    UUID   NOT NULL,
    request_digest  TEXT   NOT NULL,
    resource_id     UUID   NOT NULL,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_replay_receipts_scope_operation_id UNIQUE (scope, operation_id)
);

CREATE INDEX idx_replay_receipts_created_at_id ON setup.replay_receipts (created_at, id);

CREATE FUNCTION setup.replay_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'replay_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_replay_receipts_refuse_update
    BEFORE UPDATE ON setup.replay_receipts
    FOR EACH ROW EXECUTE FUNCTION setup.replay_receipts_refuse_update();

GRANT SELECT, INSERT, DELETE ON setup.replay_receipts TO api_runtime;
