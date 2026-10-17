-- Every authoring retry key (rule 6): create draft, send message, publish. The same
-- key and body inside 24 hours of `created_at` return the first result
-- (`REPLAY_TOMBSTONE_MS`); a different body or user is refused; a key whose
-- resource is gone answers Gone. After the window the key is new and the sweep
-- deletes the row. Append-only (rule 9).
--
-- No foreign keys: a retry key must outlive erasure of the thing it created, or a
-- late retry would silently create it again.

CREATE TABLE authoring.replay_receipts (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_replay_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id    UUID   NOT NULL,
    user_id         UUID   NOT NULL,
    scope           TEXT   NOT NULL,
    operation_id    UUID   NOT NULL,
    request_digest  TEXT   NOT NULL,
    resource_id     UUID   NOT NULL,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_replay_receipts_scope_operation_id UNIQUE (scope, operation_id)
);

-- The fixed-age sweep, and the message API's join back to its retry key.
CREATE INDEX idx_replay_receipts_created_at_id ON authoring.replay_receipts (created_at, id);
CREATE INDEX idx_replay_receipts_resource_id ON authoring.replay_receipts (resource_id);

CREATE FUNCTION authoring.replay_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'replay_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_replay_receipts_refuse_update
    BEFORE UPDATE ON authoring.replay_receipts
    FOR EACH ROW EXECUTE FUNCTION authoring.replay_receipts_refuse_update();

GRANT SELECT, INSERT, DELETE ON authoring.replay_receipts TO api_runtime;
