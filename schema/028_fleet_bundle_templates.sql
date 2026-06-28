-- First-party Fleet Bundle template catalog (curated, global).
-- Metadata only — the SKILL.md/TRIGGER.md content is fetched from the template's
-- source repo at a ref and snapshotted at import time; this table is the
-- shop-window the dashboard gallery + GET /v1/fleets/bundles read. The declared
-- credentials/tools/network are preview hints; the import re-derives the
-- authoritative requirements from the bundle's TRIGGER.md.
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
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- api_runtime serves the catalog (GET /v1/fleets/bundles). Read-only: the
-- catalog is curated through migrations, not mutated at runtime.
GRANT SELECT ON core.fleet_bundle_templates TO api_runtime;

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
