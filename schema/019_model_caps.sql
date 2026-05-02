-- Model → context-window + per-token-rate catalogue. Public, unauthenticated
-- read served via the cryptic-prefix endpoint (handlers/model_caps.zig). Both
-- the install-skill (platform-managed posture) and `zombiectl tenant provider
-- set` (BYOK posture) call the endpoint exactly once at provisioning time and
-- pin the cap into the right place. The agent runtime never reads this table
-- directly.
--
-- Cap or rate updates ship via new migrations using ON CONFLICT (model_id) DO
-- UPDATE. Never edit a released migration after it ships — bump a new slot.
--
-- The provider hosting a given model is encoded in the model_id itself
-- (`accounts/fireworks/models/...` is Fireworks; bare `kimi-k2.6` is Moonshot;
-- `claude-*` is Anthropic; etc.). Tenants pick their provider via a user-named
-- credential body, not via this catalogue.
--
-- Token rates are charged only under platform-managed posture; BYOK pays a
-- flat overhead and is billed by the user's own provider account. Models that
-- are BYOK-only at the platform tier carry zero rates here — those zeros never
-- enter the cost path because BYOK uses the flat overhead.

CREATE TABLE IF NOT EXISTS core.model_caps (
    model_id                TEXT    PRIMARY KEY,
    context_cap_tokens      INTEGER NOT NULL,
    input_cents_per_mtok    INTEGER NOT NULL,
    output_cents_per_mtok   INTEGER NOT NULL,
    created_at_ms           BIGINT  NOT NULL,
    updated_at_ms           BIGINT  NOT NULL
);

-- Seed catalogue. Caps and rates reflect each model's documented values at
-- seed time. The created_at_ms / updated_at_ms values use a fixed epoch so
-- re-running migrations produces deterministic state.
INSERT INTO core.model_caps
    (model_id, context_cap_tokens, input_cents_per_mtok, output_cents_per_mtok, created_at_ms, updated_at_ms)
VALUES
    ('claude-opus-4-7',                            1000000, 1500, 7500, 1745884800000, 1745884800000),
    ('claude-sonnet-4-6',                           256000,  300, 1500, 1745884800000, 1745884800000),
    ('claude-haiku-4-5-20251001',                   256000,  100,  500, 1745884800000, 1745884800000),
    ('gpt-5.5',                                     256000,    0,    0, 1745884800000, 1745884800000),
    ('gpt-5.4',                                     256000,    0,    0, 1745884800000, 1745884800000),
    ('kimi-k2.6',                                   256000,    0,    0, 1745884800000, 1745884800000),
    ('accounts/fireworks/models/kimi-k2.6',         256000,  150,  600, 1745884800000, 1745884800000),
    ('accounts/fireworks/models/deepseek-v4-pro',   256000,    0,    0, 1745884800000, 1745884800000),
    ('glm-5.1',                                     128000,    0,    0, 1745884800000, 1745884800000)
ON CONFLICT (model_id) DO UPDATE SET
    context_cap_tokens    = EXCLUDED.context_cap_tokens,
    input_cents_per_mtok  = EXCLUDED.input_cents_per_mtok,
    output_cents_per_mtok = EXCLUDED.output_cents_per_mtok,
    updated_at_ms         = EXCLUDED.updated_at_ms;

-- api_runtime serves the public read endpoint and the rate-cache populator
-- at API server boot. No worker access — the worker never queries this table
-- directly; tenant_providers carries the resolved cap under BYOK, frontmatter
-- carries it under platform-managed.
GRANT SELECT ON core.model_caps TO api_runtime;
