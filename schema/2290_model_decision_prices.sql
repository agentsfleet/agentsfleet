-- Decision quotes share catalogue ownership and price values with coding
-- quotes. Separate storage leaves deployed coding writers and their conflict
-- target valid while a release migrates and replaces serving daemons.
CREATE TABLE model.decision_prices (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_decision_prices_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    platform_model_id UUID NOT NULL,
    decision_revision_id UUID NOT NULL,
    effective_from_at TIMESTAMPTZ(3) NOT NULL,
    effective_until_at TIMESTAMPTZ(3),
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
    CONSTRAINT uq_decision_prices_model_revision_tier_date
        UNIQUE (platform_model_id, decision_revision_id, prompt_tokens_above, effective_from_at),
    CONSTRAINT fk_decision_prices_revision
        FOREIGN KEY (decision_revision_id, platform_model_id)
        REFERENCES model.decision_revisions (id, platform_model_id) ON DELETE CASCADE,
    CONSTRAINT ck_decision_prices_dates
        CHECK (effective_until_at IS NULL OR effective_until_at > effective_from_at),
    CONSTRAINT ck_decision_prices_nonnegative
        CHECK (prompt_tokens_above >= 0 AND input_nanos_per_mtok >= 0
            AND cached_input_nanos_per_mtok >= 0 AND output_nanos_per_mtok >= 0
            AND cache_write_nanos_per_mtok >= 0
            AND cache_write_5m_nanos_per_mtok >= 0 AND cache_write_1h_nanos_per_mtok >= 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.decision_prices TO api_runtime;
