-- First-party Fleet Library catalog (curated, global).
-- Metadata + onboarding snapshot: the SKILL.md/TRIGGER.md content and support
-- manifest are fetched from the entry's source repo at onboarding time and
-- stored here; this table is the shop-window the dashboard gallery +
-- GET /v1/fleets/bundles read. The declared credentials/tools/network are
-- preview hints; the import re-derives the authoritative requirements from the
-- bundle's TRIGGER.md.
--
-- Runtime-onboardable (M103): a platform operator holding the
-- platform-library:write scope can onboard entries at runtime via
-- POST /v1/admin/fleet-library. Seed rows (below) bootstrap the catalog;
-- onboarding populates content_hash, skill_markdown, trigger_markdown, and
-- support_files_json. The GRANT includes INSERT/UPDATE for the onboarding
-- path; writes are gated in-handler by the scope check.
--
-- Layout decision (eng-review 2026-06-20, FINAL): ONE GIT REPO PER
-- ENTRY, named agentsfleet/<id> (repo name == entry id). The repo ROOT is
-- the bundle (SKILL.md at root, optional TRIGGER.md, support files incl.
-- subfolders), so source_path is empty and the importer just strips the single
-- tarball wrapper dir — no subpath filter. Fetch is a cold path (import-time,
-- R2-cached by content hash after). source_ref is 'main' until the repos are
-- finalized; a follow-up migration pins each to a commit SHA (codex P1).
--
-- Keyed by slug (the stable API id == repo name) rather than a UUIDv7: this is a
-- curated reference catalog, not a per-tenant entity. Value sets (visibility)
-- are enforced in application code per RULE STS (no static-string DEFAULT/CHECK).
-- Onboarding columns (content_hash, skill_markdown, trigger_markdown,
-- support_files_json) are nullable: seed rows start without them and are
-- populated by the onboarding route.
CREATE TABLE IF NOT EXISTS core.fleet_library (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    description          TEXT NOT NULL,
    source_repo          TEXT NOT NULL,
    source_path          TEXT NOT NULL,
    source_ref           TEXT NOT NULL,
    required_credentials JSONB NOT NULL,
    -- Per-credential "why this fleet needs it" copy, keyed by credential name
    -- (e.g. {"github":"review your pull requests"}). A display-only preview hint
    -- the install gate renders so the operator knows why to connect — NOT a
    -- security control; credential validation reads required_credentials.
    required_credentials_reasons JSONB NOT NULL,
    required_tools       JSONB NOT NULL,
    network_hosts        JSONB NOT NULL,
    visibility           TEXT NOT NULL,
    -- Onboarding snapshot (M103): populated by POST /v1/admin/fleet-library.
    -- content_hash points to the R2 tar (fleet-bundles/sha256/{hash}.tar);
    -- support_files_json stores a path/size/hash manifest (no body content).
    content_hash         TEXT,
    skill_markdown       TEXT,
    trigger_markdown     TEXT,
    support_files_json   JSONB,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- api_runtime serves the catalog (GET /v1/fleets/bundles) and onboards entries
-- (POST /v1/admin/fleet-library). Writes are gated in-handler by the
-- platform-library:write scope (requireScope middleware).
GRANT SELECT, INSERT, UPDATE ON core.fleet_library TO api_runtime;

-- Primer: the first-party bundles, one repo each (agentsfleet/<id>). The id is
-- the identity — it names the repository, the `name:` both SKILL.md and
-- TRIGGER.md declare, and this row; the importer takes the catalog id straight
-- from that frontmatter name, so a bundle whose name drifts from its repo
-- onboards as a second entry instead of filling the row seeded here.
--
-- These rows carry only the curated metadata an operator cannot derive from a
-- bundle: the description and the per-credential "why this fleet needs it" copy
-- the install gate shows. They stay invisible in the gallery until a platform
-- operator onboards the repository (the list query filters on
-- `content_hash IS NOT NULL`), which upserts the bundle's hash, markdown, and
-- re-derived tools onto the row.
--
-- source_path empty (repo root is the bundle). source_ref 'main' until the
-- repos are finalized — pin to a commit SHA once they settle.
-- ON CONFLICT keeps the seed idempotent on re-apply.
INSERT INTO core.fleet_library
    (id, name, description, source_repo, source_path, source_ref,
     required_credentials, required_credentials_reasons, required_tools, network_hosts, visibility,
     created_at, updated_at)
VALUES
    ('github-pr-reviewer',
     'GitHub Pull Request reviewer',
     'Reviews GitHub pull requests and posts review comments.',
     'agentsfleet/github-pr-reviewer', '', 'main',
     '["github"]'::jsonb,
     '{"github":"review your pull requests and post review comments"}'::jsonb,
     '["http_request"]'::jsonb,
     '["api.github.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint),
    ('zoho-sprint-daily-summarizer',
     'Zoho Sprints daily summarizer',
     'Summarizes the day''s Zoho Sprints activity and posts a digest.',
     'agentsfleet/zoho-sprint-daily-summarizer', '', 'main',
     '["zoho"]'::jsonb,
     '{"zoho":"read your Zoho Sprints activity for the daily digest"}'::jsonb,
     '["http_request"]'::jsonb,
     '["sprintsapi.zoho.com","accounts.zoho.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint),
    ('zoho-recruiter-daily-summarizer',
     'Zoho Recruit daily summarizer',
     'Summarizes the day''s Zoho Recruit pipeline activity and posts a digest.',
     'agentsfleet/zoho-recruiter-daily-summarizer', '', 'main',
     '["zoho_recruit"]'::jsonb,
     '{"zoho_recruit":"read your hiring pipeline for the daily digest"}'::jsonb,
     '["http_request"]'::jsonb,
     '["recruit.zoho.com","accounts.zoho.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint),
    ('platform-ops',
     'Platform operations diagnostician',
     'Reads Fly.io app state and logs and Upstash Redis stats, correlates them into one cause, and posts the diagnosis to Slack.',
     'agentsfleet/platform-ops', '', 'main',
     '["fly","upstash","slack","github"]'::jsonb,
     '{"fly":"read your app state and logs","upstash":"read your Redis database stats","slack":"post the diagnosis to your channel","github":"read the failed workflow run and its commits"}'::jsonb,
     '["http_request"]'::jsonb,
     '["api.machines.dev","api.upstash.com","slack.com","api.github.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint)
ON CONFLICT (id) DO NOTHING;
