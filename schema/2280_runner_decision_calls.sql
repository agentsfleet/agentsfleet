-- Physical decision accounting outlives lease, admission, task, fleet and model
-- deletion. These opaque attribution ids deliberately have no foreign keys.
-- Private request/answer data lives in the separately cascading payload table.
CREATE TABLE runner.decision_calls (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_decision_calls_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    original_runner_id UUID NOT NULL,
    original_lease_id UUID NOT NULL,
    source_admission_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    -- Budget ownership only: intent is charged to its source admission even
    -- while another task holds the lease. Its execution stamp stays below.
    task_id UUID,
    task_generation BIGINT,
    fencing_token BIGINT NOT NULL CHECK (fencing_token >= 0),
    operation_id UUID NOT NULL,
    request_digest TEXT NOT NULL,
    assessment_id UUID NOT NULL,
    assessment_key TEXT NOT NULL,
    attempt_number SMALLINT NOT NULL CHECK (attempt_number >= 0 AND attempt_number <= 1),
    retry_of_decision_call_id UUID REFERENCES runner.decision_calls (id),
    logical_expires_at TIMESTAMPTZ(3) NOT NULL,
    purpose TEXT NOT NULL,
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    fleet_name TEXT NOT NULL,
    profile_snapshot JSONB NOT NULL,
    price_snapshot JSONB NOT NULL,
    -- Non-secret policy/task generations and input/effect cursors assessed by
    -- this call. The recorder compares semantic identity without usage counters.
    context_snapshot JSONB NOT NULL,
    -- Canonical range: afd_wire::usage::DECISION_SEQ_BASE through u32::MAX.
    -- The native allocator proves the decision range before insertion.
    usage_sequence BIGINT NOT NULL CHECK (usage_sequence >= 0),
    reserved_nanos BIGINT NOT NULL CHECK (reserved_nanos >= 0),
    -- NULL until native dispatch issuance commits; it never becomes NULL again.
    issued_at TIMESTAMPTZ(3),
    -- NULL while funding or physical usage is unresolved. Read by hold release.
    accounted_at TIMESTAMPTZ(3),
    known_usage JSONB,
    charged_nanos BIGINT CHECK (charged_nanos >= 0),
    canonical_receipt_digest TEXT,
    -- A first unknown outcome cannot later be rewritten into retry evidence.
    failure_receipt_digest TEXT,
    failure_reason JSONB,
    failure_recorded_at TIMESTAMPTZ(3),
    reported_model TEXT,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_decision_calls_scope UNIQUE (id, fleet_id, workspace_id, tenant_id),
    CONSTRAINT uq_decision_calls_operation UNIQUE (original_lease_id, operation_id),
    CONSTRAINT uq_decision_calls_assessment UNIQUE (assessment_key, attempt_number),
    CONSTRAINT uq_decision_calls_retry UNIQUE (retry_of_decision_call_id),
    CONSTRAINT uq_decision_calls_usage UNIQUE (original_lease_id, usage_sequence),
    CONSTRAINT ck_decision_calls_attempt CHECK (
        (attempt_number = 0 AND retry_of_decision_call_id IS NULL)
        OR (attempt_number = 1 AND retry_of_decision_call_id IS NOT NULL)
    ),
    CONSTRAINT ck_decision_calls_task_generation CHECK (
        (task_id IS NULL AND task_generation IS NULL)
        OR (task_id IS NOT NULL AND task_generation IS NOT NULL AND task_generation > 0)
    ),
    CONSTRAINT ck_decision_calls_known_usage CHECK (known_usage IS NULL OR issued_at IS NOT NULL),
    CONSTRAINT ck_decision_calls_charge_accounted CHECK (charged_nanos IS NULL OR accounted_at IS NOT NULL)
);

-- Renewal, final report and the billing sweeper share this unresolved-work lookup.
CREATE INDEX idx_decision_calls_unresolved_lease
    ON runner.decision_calls (original_lease_id) WHERE accounted_at IS NULL;
CREATE INDEX idx_decision_calls_unresolved_fleet
    ON runner.decision_calls (fleet_id, workspace_id) WHERE accounted_at IS NULL;
-- Task/source bounds include reservations across replacement leases.
CREATE INDEX idx_decision_calls_task ON runner.decision_calls (task_id, created_at);
CREATE INDEX idx_decision_calls_source ON runner.decision_calls (source_admission_id, created_at);
CREATE INDEX idx_decision_calls_overage_fleet ON runner.decision_calls (fleet_id, workspace_id)
    WHERE charged_nanos > reserved_nanos;

GRANT SELECT, INSERT, UPDATE ON runner.decision_calls TO api_runtime;
