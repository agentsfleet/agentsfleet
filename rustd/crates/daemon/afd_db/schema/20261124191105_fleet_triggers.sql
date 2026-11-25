-- What may wake a fleet besides a message sent to it: a signed webhook delivery, an event from the
-- workspace's GitHub App, or a mention in one Slack channel. The fleet adds a row by tool and a
-- person removes it; a row is never edited, so a change is a remove and an add. Rows, not a
-- column, because a fleet holds several and the three ingress statements route a delivery by
-- them (`afd_ingress`, `afd_api_ingress/src/handler/{webhook,mention}`), each keeping its
-- `workspace_id` predicate so a delivery never reaches another workspace's fleet.
--
-- `trigger_type` is `webhook`, `app_event` or `mention`, spelled by
-- `af_wire::trigger_verb::TriggerKind`; `source` is the webhook's URL slug, `github` or `slack`.
-- One row per (fleet, trigger_type, source): one webhook per source, one App subscription and one
-- channel per fleet. `events` and `repositories` narrow an App event or a webhook, NULL meaning
-- every one. A webhook from a source outside the built-in registry carries its signing recipe
-- (`verifier_scheme`, and `verifier_header` for a recipe that signs the body alone).
-- `secret_name` is the workspace vault key of the signing secret, never the secret. `channel` is
-- a mention's channel identifier.
CREATE TABLE fleet.triggers (
    id                  UUID PRIMARY KEY,
    fleet_id            UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id        UUID    NOT NULL,
    trigger_type        TEXT    NOT NULL,
    source              TEXT    NOT NULL,
    events              TEXT[],
    repositories        TEXT[],
    verifier_scheme     TEXT,
    verifier_header     TEXT,
    secret_name         TEXT,
    channel             TEXT,
    created_at          TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_triggers_fleet_id_trigger_type_source UNIQUE (fleet_id, trigger_type, source)
);

-- A webhook or App delivery resolves its fleets by workspace, trigger type and source.
CREATE INDEX idx_triggers_workspace_id_trigger_type_source
    ON fleet.triggers (workspace_id, trigger_type, source);

-- A mention resolves the fleets subscribed to its channel.
CREATE INDEX idx_triggers_workspace_id_trigger_type_channel
    ON fleet.triggers (workspace_id, trigger_type, channel);

-- Append-only: a change is a remove and an add, so even a role granted UPDATE by mistake is
-- refused one. DELETE stays granted, and the fleet's delete still cascades.
CREATE FUNCTION fleet.triggers_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'triggers is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_triggers_refuse_update
    BEFORE UPDATE ON fleet.triggers
    FOR EACH ROW EXECUTE FUNCTION fleet.triggers_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.triggers TO api_runtime;
