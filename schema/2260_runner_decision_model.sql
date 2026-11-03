-- The operator assigns a catalogue identity, never provider credentials.
-- Runner self, heartbeat and operator views read this optional selection.
-- Referenced models remain until operators remove their assignments.
ALTER TABLE runner.runners
    ADD COLUMN decision_platform_model_id UUID
        REFERENCES model.platform_models (id) ON DELETE RESTRICT;

CREATE INDEX idx_runners_decision_platform_model
    ON runner.runners (decision_platform_model_id)
    WHERE decision_platform_model_id IS NOT NULL;
