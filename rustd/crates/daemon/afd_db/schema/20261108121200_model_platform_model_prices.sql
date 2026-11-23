-- Rates are integer nanodollars per million tokens. A NULL cache-write rate means unverified or
-- unsupported, never free; NULL `effective_until` is an open range. The unqualified cache-write
-- rate has no reported lifetime and is distinct from the 5-minute and 1-hour rates.
-- A NULL `decision_revision_id` is a coding (chat) price, read by the price book, the credential
-- registry, setup and the tenant library. A decision quote names the revision it prices, read by
-- `afd_billing`'s decision price book and the admin catalogue; the composite key proves that
-- revision belongs to the same model. The unique key treats NULLs as equal, so each schedule
-- holds one row per tier and start.
CREATE TABLE model.platform_model_prices (
    id UUID PRIMARY KEY,
    platform_model_id UUID NOT NULL REFERENCES model.platform_models(id) ON DELETE CASCADE,
    decision_revision_id UUID,
    effective_from TIMESTAMPTZ(3) NOT NULL,
    effective_until TIMESTAMPTZ(3),
    prompt_tokens_above INTEGER NOT NULL,
    input_nanos_per_mtok BIGINT NOT NULL,
    cached_input_nanos_per_mtok BIGINT NOT NULL,
    cache_write_nanos_per_mtok BIGINT,
    cache_write_5m_nanos_per_mtok BIGINT,
    cache_write_1h_nanos_per_mtok BIGINT,
    output_nanos_per_mtok BIGINT NOT NULL,
    source_url TEXT NOT NULL,
    verified_at TIMESTAMPTZ(3) NOT NULL,
    conditions TEXT NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_platform_model_prices_model_revision_tier_date
        UNIQUE NULLS NOT DISTINCT
            (platform_model_id, decision_revision_id, prompt_tokens_above, effective_from),
    CONSTRAINT fk_platform_model_prices_decision_revision
        FOREIGN KEY (decision_revision_id, platform_model_id)
        REFERENCES model.decision_revisions (id, platform_model_id) ON DELETE CASCADE,
    CONSTRAINT ck_platform_model_prices_dates
        CHECK (effective_until IS NULL OR effective_until > effective_from),
    CONSTRAINT ck_platform_model_prices_nonnegative
        CHECK (prompt_tokens_above >= 0 AND input_nanos_per_mtok >= 0
            AND cached_input_nanos_per_mtok >= 0 AND output_nanos_per_mtok >= 0
            AND cache_write_nanos_per_mtok >= 0
            AND cache_write_5m_nanos_per_mtok >= 0 AND cache_write_1h_nanos_per_mtok >= 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_model_prices TO api_runtime;
