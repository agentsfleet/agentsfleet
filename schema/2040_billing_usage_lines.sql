-- One priced provider request (or runtime-only slice) of a piece of billed work:
-- a fleet lease or a setup-chat attempt. Written in the work's own transaction,
-- so the work and what it cost commit together; the biller moves money after
-- commit and stamps `settled_at`. The tenant's charges page and the fleet budget
-- read this table, whichever biller is configured.
--
-- No foreign keys: a line outlives the fleet, workspace, tenant and price row it
-- names (deletion matrix), so every reference is an opaque value and
-- `fleet_name` is the name when the line was written. `work_type` spellings and
-- `posture` spellings are application constants (RULE STS). Exactly one of
-- `fleet_id` / `authoring_attempt_id` is set, by `work_type`; `event_id` is set
-- for fleet work. `platform_model_price_id` is NULL when the catalogue had no
-- price covering the request, and `unpriced_token_count` counts tokens in any
-- category the chosen price leaves unknown, so an unpriced token is visible
-- rather than silently free.

CREATE TABLE billing.usage_lines (
    id                          UUID   PRIMARY KEY,
    CONSTRAINT ck_usage_lines_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    work_type                   TEXT   NOT NULL,
    work_id                     UUID   NOT NULL,
    seq                         INTEGER NOT NULL,
    tenant_id                   UUID   NOT NULL,
    workspace_id                UUID   NOT NULL,
    fleet_id                    UUID,
    fleet_name                  TEXT,
    event_id                    TEXT,
    authoring_attempt_id        UUID,
    provider                    TEXT   NOT NULL,
    model                       TEXT   NOT NULL,
    posture                     TEXT   NOT NULL,
    requested_at                TIMESTAMPTZ(3) NOT NULL,
    input_token_count           BIGINT NOT NULL,
    cached_input_token_count    BIGINT NOT NULL,
    cache_write_token_count     BIGINT NOT NULL,
    cache_write_5m_token_count  BIGINT NOT NULL,
    cache_write_1h_token_count  BIGINT NOT NULL,
    output_token_count          BIGINT NOT NULL,
    runtime_ms                  BIGINT NOT NULL,
    platform_model_price_id     UUID,
    unpriced_token_count        BIGINT NOT NULL,
    charged_nanos               BIGINT NOT NULL,
    settled_at                  TIMESTAMPTZ(3),
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_usage_lines_work_type_work_id_seq UNIQUE (work_type, work_id, seq),
    CONSTRAINT ck_usage_lines_non_negative
        CHECK (seq >= 0 AND input_token_count >= 0 AND cached_input_token_count >= 0
            AND cache_write_token_count >= 0 AND cache_write_5m_token_count >= 0
            AND cache_write_1h_token_count >= 0 AND output_token_count >= 0
            AND runtime_ms >= 0 AND unpriced_token_count >= 0 AND charged_nanos >= 0)
);

-- The tenant's charges page, newest first.
CREATE INDEX idx_usage_lines_tenant_id_created_at_id
    ON billing.usage_lines (tenant_id, created_at DESC, id DESC);
-- The fleet budget's day and month sums.
CREATE INDEX idx_usage_lines_fleet_id_created_at
    ON billing.usage_lines (fleet_id, created_at) WHERE fleet_id IS NOT NULL;
-- The sweeper: lines committed whose settlement never landed.
CREATE INDEX idx_usage_lines_created_at_unsettled
    ON billing.usage_lines (created_at) WHERE settled_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON billing.usage_lines TO api_runtime;
