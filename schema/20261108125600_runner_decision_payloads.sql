-- One physical call's bounded private evidence and answer. Keeping these in a
-- child permits privacy deletion without erasing paid-call attribution/usage.
CREATE TABLE runner.decision_payloads (
    decision_call_id UUID PRIMARY KEY,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    task_id UUID,
    request JSONB NOT NULL,
    result JSONB,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_decision_payloads_call_scope
        FOREIGN KEY (decision_call_id, fleet_id, workspace_id, tenant_id)
        REFERENCES runner.decision_calls (id, fleet_id, workspace_id, tenant_id),
    CONSTRAINT fk_decision_payloads_fleet_scope
        FOREIGN KEY (fleet_id, workspace_id, tenant_id)
        REFERENCES fleet.fleets (id, workspace_id, tenant_id) ON DELETE CASCADE,
    CONSTRAINT fk_decision_payloads_task_scope
        FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE CASCADE
);

-- Both owner-deletion cascades must find their payloads without a table scan.
CREATE INDEX idx_decision_payloads_fleet ON runner.decision_payloads (fleet_id);

CREATE INDEX idx_decision_payloads_task ON runner.decision_payloads (task_id)
    WHERE task_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.decision_payloads TO api_runtime;
