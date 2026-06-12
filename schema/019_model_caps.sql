-- Model → context-window + per-token-rate catalogue. Public, unauthenticated
-- read served via the cryptic-prefix endpoint (handlers/model_caps.zig). Both
-- the install-skill (platform-managed posture) and `zombiectl tenant provider
-- set` (self-managed posture) call the endpoint exactly once at provisioning time and
-- pin the cap into the right place. The agent runtime never reads this table
-- directly.
--
-- Cap or rate updates ship via new migrations using ON CONFLICT (model_id) DO
-- UPDATE. Never edit a released migration after it ships — bump a new slot.
--
-- The provider hosting a given model is carried explicitly in the `provider`
-- column (anthropic | fireworks | minimax | pioneer | openai | moonshot | …).
-- The same base model can appear under more than one provider at different
-- rates (e.g. Claude Haiku 4.5 direct from Anthropic vs hosted on Pioneer), so
-- each (provider, model) pair is its own row with its own model_id. Tenants
-- pick their provider via a user-named credential body, not via this catalogue.
-- Provider values are app-enforced (named constants), not a SQL CHECK (RULE STS).
--
-- Token rates are charged only under platform-managed posture; self-managed
-- pays the run fee only and is billed by the user's own provider account.
-- Models that are self-managed-only at the platform tier carry zero rates
-- here — those zeros never enter the cost path because self-managed charges
-- no token cost at all.
--
-- Three priced tiers per model: fresh input, cached input (a prompt-cache
-- read — materially cheaper, ~10% of fresh input), and output. The cached
-- tier mirrors provider pricing (Fireworks-style input / cached-input /
-- output). A self-managed-only model carries zero across all three.
--
-- Rates are expressed in nanos per million tokens (1 nano = 1/1,000,000,000
-- USD). Type is BIGINT because $30/M tokens in nanos = 3e10, beyond INT32_MAX.

CREATE TABLE IF NOT EXISTS core.model_caps (
    uid                            UUID    PRIMARY KEY,
    CONSTRAINT ck_model_caps_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    model_id                       TEXT    NOT NULL,
    provider                       TEXT    NOT NULL,
    context_cap_tokens             INTEGER NOT NULL,
    input_nanos_per_mtok           BIGINT  NOT NULL,
    cached_input_nanos_per_mtok    BIGINT  NOT NULL,
    output_nanos_per_mtok          BIGINT  NOT NULL,
    created_at_ms                  BIGINT  NOT NULL,
    updated_at_ms                  BIGINT  NOT NULL,
    -- Unique domain key: the same base model is hosted by more than one provider
    -- at different rates (e.g. claude-opus-4-8 direct from Anthropic vs on
    -- Pioneer), so (provider, model_id) — not model_id alone — identifies a row.
    CONSTRAINT uq_model_caps_provider_model UNIQUE (provider, model_id)
);

-- Seed catalogue. Caps and rates reflect each model's documented values at
-- seed time, scaled cents → nanos × 10,000,000 (1¢ = 10M nanos). The
-- created_at_ms / updated_at_ms values use a fixed epoch so re-running
-- migrations produces deterministic state.
-- Rates in nanos/Mtok. cached_input seeded at ~10% of fresh input where the
-- provider publishes no cache rate (a prompt-cache read costs roughly a tenth
-- of an uncached read); self-managed-only models carry zero across all tiers.
-- The same base model can recur under a different provider at a different rate
-- (e.g. Claude Opus 4.8 direct from Anthropic vs hosted on Pioneer) — each is
-- its own row, keyed by a provider-distinct model_id.
INSERT INTO core.model_caps
    (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
VALUES
    ('0195b4ba-8d3a-7f13-8abc-000000000191', 'claude-opus-4-8',                            'anthropic', 1000000,  5000000000,  500000000, 25000000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000192', 'claude-sonnet-4-6',                          'anthropic',  256000,  3000000000,  300000000, 15000000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000193', 'claude-haiku-4-5-20251001',                  'anthropic',  256000,  1000000000,  100000000,  5000000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000194', 'gpt-5.5',                                    'openai',     256000,           0,          0,           0, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000195', 'gpt-5.4',                                    'openai',     256000,           0,          0,           0, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000196', 'kimi-k2.6',                                  'moonshot',   256000,           0,          0,           0, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000197', 'accounts/fireworks/models/kimi-k2.6',        'fireworks',  256000,   950000000,  100000000,  4000000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000198', 'accounts/fireworks/models/deepseek-v4-pro',  'fireworks',  256000,  1740000000,  140000000,  3480000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-000000000199', 'minimax-m3',                                 'minimax',   1048576,   600000000,   60000000,  2400000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-00000000019a', 'claude-opus-4-8',                            'pioneer',   1000000,  5500000000,  550000000, 27500000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-00000000019b', 'claude-sonnet-4-6',                          'pioneer',   1000000,  3300000000,  330000000, 16500000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-00000000019c', 'claude-haiku-4-5',                           'pioneer',    200000,  1100000000,  110000000,  5500000000, 1745884800000, 1745884800000),
    ('0195b4ba-8d3a-7f13-8abc-00000000019d', 'moonshotai/Kimi-K2.6',                       'pioneer',    262000,   950000000,   95000000,  4000000000, 1745884800000, 1745884800000)
ON CONFLICT (provider, model_id) DO UPDATE SET
    provider                    = EXCLUDED.provider,
    context_cap_tokens          = EXCLUDED.context_cap_tokens,
    input_nanos_per_mtok        = EXCLUDED.input_nanos_per_mtok,
    cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
    output_nanos_per_mtok       = EXCLUDED.output_nanos_per_mtok,
    updated_at_ms               = EXCLUDED.updated_at_ms;

-- api_runtime serves the public read endpoint and the rate-cache populator
-- at API server boot. No worker access — the worker never queries this table
-- directly; tenant_providers carries the resolved cap under self-managed, frontmatter
-- carries it under platform-managed.
GRANT SELECT ON core.model_caps TO api_runtime;
