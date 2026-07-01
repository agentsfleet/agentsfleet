-- First-party Fleet Bundle template catalog (curated, global).
-- Metadata + onboarding snapshot: the SKILL.md/TRIGGER.md content and support
-- manifest are fetched from the template's source repo at onboarding time and
-- stored here; this table is the shop-window the dashboard gallery +
-- GET /v1/fleets/bundles read. The declared credentials/tools/network are
-- preview hints; the import re-derives the authoritative requirements from the
-- bundle's TRIGGER.md.
--
-- Runtime-onboardable (M103): a platform operator holding the
-- platform-template:write scope can onboard templates at runtime via
-- POST /v1/admin/fleet-templates. Seed rows (below) bootstrap the catalog;
-- onboarding populates content_hash, skill_markdown, trigger_markdown, and
-- support_files_json. The GRANT includes INSERT/UPDATE for the onboarding
-- path; writes are gated in-handler by the scope check.
--
-- Layout decision (eng-review 2026-06-20, FINAL): ONE GIT REPO PER
-- TEMPLATE, named agentsfleet/<id> (repo name == template id). The repo ROOT is
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
CREATE TABLE IF NOT EXISTS core.fleet_bundle_templates (
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
    -- Onboarding snapshot (M103): populated by POST /v1/admin/fleet-templates.
    -- content_hash points to the R2 tar (fleet-bundles/sha256/{hash}.tar);
    -- support_files_json stores a path/size/hash manifest (no body content).
    content_hash         TEXT,
    skill_markdown       TEXT,
    trigger_markdown     TEXT,
    support_files_json   JSONB,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- api_runtime serves the catalog (GET /v1/fleets/bundles) and onboards templates
-- (POST /v1/admin/fleet-templates). Writes are gated in-handler by the
-- platform-template:write scope (requireScope middleware).
GRANT SELECT, INSERT, UPDATE ON core.fleet_bundle_templates TO api_runtime;

-- Primer: three first-party templates, one repo each (agentsfleet/<id>).
-- source_path empty (repo root is the bundle). source_ref 'main' until the
-- repos are finalized — pin to a commit SHA in a follow-up migration.
-- ON CONFLICT keeps the seed idempotent on re-apply.
INSERT INTO core.fleet_bundle_templates
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
     '["github_review_comment"]'::jsonb,
     '["api.github.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint),
    ('zoho-sprint-daily-summarizer',
     'Zoho Sprints daily summarizer',
     'Summarizes the day''s Zoho Sprints activity and posts a digest.',
     'agentsfleet/zoho-sprint-daily-summarizer', '', 'main',
     '["zoho"]'::jsonb,
     '{"zoho":"read your Zoho Sprints activity for the daily digest"}'::jsonb,
     '["zoho_sprint_read"]'::jsonb,
     '["sprintsapi.zoho.com","accounts.zoho.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint),
    ('security-reviewer',
     'Security reviewer',
     'Reviews pull requests for security issues and posts findings.',
     'agentsfleet/security-reviewer', '', 'main',
     '["github"]'::jsonb,
     '{"github":"scan your pull requests for security issues and post findings"}'::jsonb,
     '["github_review_comment"]'::jsonb,
     '["api.github.com"]'::jsonb, 'public',
     (extract(epoch from now()) * 1000)::bigint,
     (extract(epoch from now()) * 1000)::bigint)
ON CONFLICT (id) DO NOTHING;
