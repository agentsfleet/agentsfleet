-- Every retry key: a fleet's create and a checkpoint allocation (a turn's or a durable task's
-- capture plan). `scope` names the workspace and the owner the key belongs to
-- (`<workspace>:turn:<turn>:checkpoint`, `<workspace>:fleet:create:<person>`), so
-- `(scope, operation_id)` is the natural key every statement addresses. `request_digest` covers
-- the request and, for a create, its actor; `resource_id` is what the first request created. A
-- create answers a key for its replay window and then reuses it; a checkpoint key answers for
-- its owner's life. Append-only, and no foreign keys: a key must outlive erasure of what it
-- created, or a late retry would create it again.
CREATE TABLE thread.replay_receipts (
    scope           TEXT   NOT NULL,
    operation_id    UUID   NOT NULL,
    request_digest  TEXT   NOT NULL,
    resource_id     UUID   NOT NULL,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    PRIMARY KEY (scope, operation_id)
);

CREATE FUNCTION thread.replay_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'replay_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_replay_receipts_refuse_update
    BEFORE UPDATE ON thread.replay_receipts
    FOR EACH ROW EXECUTE FUNCTION thread.replay_receipts_refuse_update();

GRANT SELECT, INSERT, DELETE ON thread.replay_receipts TO api_runtime;
