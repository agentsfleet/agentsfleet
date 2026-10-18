-- Admission, authoring and settlement read a model's prompt-tier/date price.
-- Why not columns on platform_models: a model has several prompt tiers and
-- effective date ranges; identity is one row and these prices are many rows.
-- Rates are integer nanodollars per million tokens. A missing cache-write rate
-- means unverified or unsupported, never free. NULL effective_until is an open
-- range, so an undated promotion does not acquire an invented ending.
-- Indy approved these two instant names from #771 on 2026-10-18; every other
-- instant retains the _at suffix. Unqualified cache writes have no reported
-- lifetime and remain distinct from the 5-minute and 1-hour categories.

CREATE TABLE model.platform_model_prices (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_platform_model_prices_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    platform_model_id UUID NOT NULL REFERENCES model.platform_models(id) ON DELETE CASCADE,
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
    CONSTRAINT uq_platform_model_prices_model_tier_date
        UNIQUE (platform_model_id, prompt_tokens_above, effective_from),
    CONSTRAINT ck_platform_model_prices_dates
        CHECK (effective_until IS NULL OR effective_until > effective_from),
    CONSTRAINT ck_platform_model_prices_nonnegative
        CHECK (prompt_tokens_above >= 0 AND input_nanos_per_mtok >= 0
            AND cached_input_nanos_per_mtok >= 0 AND output_nanos_per_mtok >= 0
            AND cache_write_nanos_per_mtok >= 0
            AND cache_write_5m_nanos_per_mtok >= 0 AND cache_write_1h_nanos_per_mtok >= 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON model.platform_model_prices TO api_runtime;
