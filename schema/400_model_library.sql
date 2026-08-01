-- Model → context-window + per-token-rate catalogue. Public, unauthenticated
-- read served via the cryptic-prefix endpoint (handlers/model_library.zig). Both
-- the install-skill (platform-managed posture) and `agentctl tenant provider set`
-- (self-managed posture) call the endpoint exactly once at provisioning time and
-- pin the cap into the right place. The agent runtime never reads this table.
--
-- Two columns end in `id` and mean different things, so the distinction is
-- stated rather than left to be inferred: `id` is the row's identity, minted by
-- the application as UUID version 7, and is what `/v1/admin/models/{id}`
-- addresses. `model_id` is the provider's own model name (`claude-opus-5`) — a
-- domain string, part of the published response shape, and not an identifier of
-- anything in this database.
--
-- The table therefore carries two keys, and unlike the duplicate-twin shape this
-- rebuild retires, that is correct here: they hold DIFFERENT values. The retired
-- shape paired a primary key with a unique twin over the same value, so two
-- sessions inserting a new row raced to a duplicate-key error on whichever index
-- the statement did not name. Here the admin upsert arbitrates
-- (provider, model_id) while `id` is freshly minted per attempt and cannot
-- collide, so the update arm is reached as intended.
--
-- This table ships EMPTY — no seed. Platform admins populate and maintain the
-- catalogue through the admin model-caps API (`/v1/admin/models`), which
-- repopulates the in-process rate cache live on every mutation. Earlier
-- revisions seeded a fixed 13-row catalogue here; that seed was removed once the
-- admin write surface landed, so a fresh environment starts from an
-- admin-curated rather than a migration-frozen catalogue.
--
-- The provider hosting a given model is carried explicitly in `provider`
-- (anthropic | fireworks | minimax | pioneer | openai | moonshot | …). The same
-- base model can appear under more than one provider at different rates (Claude
-- Haiku 4.5 direct from Anthropic vs hosted on Pioneer), so each
-- (provider, model_id) pair is its own row. Tenants pick their provider via a
-- user-named credential body, not via this catalogue. Provider values are
-- app-enforced named constants, never a SQL CHECK (RULE STS).
--
-- Token rates are charged only under platform-managed posture; self-managed pays
-- the run fee only and is billed by the user's own provider account. Models that
-- are self-managed-only at the platform tier carry zero rates here — those zeros
-- never enter the cost path, because self-managed charges no token cost at all.
--
-- Three priced tiers per model: fresh input, cached input (a prompt-cache read,
-- materially cheaper at roughly 10% of fresh input), and output. The cached tier
-- mirrors provider pricing (Fireworks-style input / cached-input / output).
--
-- Rates are nanos per million tokens (1 nano = 1/1,000,000,000 United States
-- Dollar). BIGINT because $30 per million tokens in nanos is 3e10, beyond the
-- 32-bit signed maximum.

CREATE TABLE IF NOT EXISTS core.model_library (
    id                          UUID    PRIMARY KEY,
    CONSTRAINT ck_model_library_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    model_id                    TEXT    NOT NULL,
    provider                    TEXT    NOT NULL,
    context_cap_tokens          INTEGER NOT NULL,
    input_nanos_per_mtok        BIGINT  NOT NULL,
    cached_input_nanos_per_mtok BIGINT  NOT NULL,
    output_nanos_per_mtok       BIGINT  NOT NULL,
    created_at                  BIGINT  NOT NULL,
    updated_at                  BIGINT  NOT NULL,
    CONSTRAINT uq_model_library_provider_model_id UNIQUE (provider, model_id)
);

-- No additional index. The unique constraint above is a btree on
-- (provider, model_id), which is how the admin upsert arbitrates, how the rate
-- cache populator loads a row, and how `core.platform_provider_defaults`
-- resolves its foreign key; its `provider` prefix serves the per-provider list.
-- The catalogue is admin-curated and small, so the full-table read that backs
-- the public endpoint is a sequential scan on purpose.

-- api_runtime serves the public read endpoint and the rate-cache populator at
-- Application Programming Interface server boot, and owns the admin model-caps
-- write surface (/v1/admin/models). No runner access — the runner never queries
-- this table: `core.tenant_model_selection` carries the resolved cap under
-- self-managed, and frontmatter carries it under platform-managed.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.model_library TO api_runtime;
