-- Model → context-window + per-token-rate catalogue. Public, unauthenticated
-- read served via the cryptic-prefix endpoint (handlers/model_caps.zig). Both
-- the install-skill (platform-managed posture) and `agentctl tenant provider
-- set` (self-managed posture) call the endpoint exactly once at provisioning time and
-- pin the cap into the right place. The agent runtime never reads this table
-- directly.
--
-- This table ships EMPTY — no seed. Platform admins populate and maintain the
-- catalogue through the admin model-caps API (`/v1/admin/models`), which
-- repopulates the in-process rate cache live on every mutation. Earlier
-- revisions seeded a fixed 13-row catalogue here; that seed was removed once
-- the admin write surface landed so a fresh environment starts from an
-- admin-curated, not migration-frozen, catalogue.
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

CREATE TABLE IF NOT EXISTS core.model_library (
    uid                            UUID    PRIMARY KEY,
    CONSTRAINT ck_model_library_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
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
    CONSTRAINT uq_model_library_provider_model UNIQUE (provider, model_id)
);

-- api_runtime serves the public read endpoint + the rate-cache populator at API
-- server boot, and the admin model-caps CRUD API (/v1/admin/models) writes the
-- catalogue. No worker access — the worker never queries this table directly;
-- tenant_providers carries the resolved cap under self-managed, frontmatter
-- carries it under platform-managed.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.model_library TO api_runtime;
