-- Every funds check sums one tenant's unsettled lines: the fleet issue and
-- renewal gates, the setup chat claim gate and each setup chat model call.
-- The partial index keeps that read to the tenant's lines the relay has not
-- yet charged, a few seconds' worth, however long the tenant's history.
CREATE INDEX idx_usage_lines_tenant_id_unsettled
    ON billing.usage_lines (tenant_id) INCLUDE (charged_nanos) WHERE settled_at IS NULL;
