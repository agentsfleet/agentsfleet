-- Reconciles the first-party catalog on a database that already applied 023.
--
-- 023 is the canonical seed and was corrected in place, per the pre-v2.0.0
-- teardown-rebuild convention (docs/SCHEMA_CONVENTIONS.md). That convention only
-- reaches a database built from scratch: the migrator records applied versions
-- in audit.schema_migrations and compares by version number, with no checksum
-- over the SQL text, so an edited 023 is skipped wherever version 23 already
-- ran. Without this file the correction never reaches an existing deployment —
-- `security-reviewer` would keep pointing at a repository that does not exist,
-- and the two new bundles would never gain their curated metadata.
--
-- Idempotent, and a no-op on a fresh database where 023 already produced this
-- exact state. Data only: no ALTER, no DROP, no schema change.

-- security-reviewer named a repository that was never published, so its row
-- could only ever fail an onboard. Scoped to a row no operator has onboarded: a
-- non-null content_hash would mean a real bundle landed here, and this delete
-- must never destroy one.
DELETE FROM core.fleet_library
 WHERE id = 'security-reviewer'
   AND content_hash IS NULL;

-- The bundles published at agentsfleet/<id>. Seed rows carry only the curated
-- metadata the importer cannot derive — the description and the per-credential
-- reason the install gate shows. They stay invisible in the gallery until an
-- operator onboards the repository (the list query filters on
-- `content_hash IS NOT NULL`), which is what fills the hash, the markdown, and
-- the re-derived tools.
--
-- ON CONFLICT touches only rows that hold NO bundle (`content_hash IS NULL`).
-- The catalog id is derived from a bundle's SKILL.md frontmatter name, so a row
-- with one of these ids may have been materialized from a DIFFERENT repository
-- whose bundle happens to declare the same name. Rewriting its source, declared
-- credentials, and network hosts while leaving that bundle's stored markdown and
-- hash in place would produce a hybrid row: the gallery and the install gate
-- would describe a first-party fleet while the runner served someone else's
-- content. Seeding curated metadata is never worth that, so a materialized row
-- is left entirely alone — reconciling it is an operator decision, not a
-- migration's.
INSERT INTO core.fleet_library
    (id, name, description, source_repo, source_path, source_ref,
     required_credentials, required_credentials_reasons, required_tools, network_hosts, visibility,
     created_at, updated_at)
VALUES
    ('platform-ops',
     'Platform operations diagnostician',
     'Reads Fly.io app state and logs and Upstash Redis stats, correlates them into one cause, and posts the diagnosis to Slack.',
     'agentsfleet/platform-ops', '', 'main',
     '["fly","upstash","slack","github"]'::jsonb,
     '{"fly":"read your app state and logs","upstash":"read your Redis database stats","slack":"post the diagnosis to your channel","github":"read the failed workflow run and its commits"}'::jsonb,
     '["http_request"]'::jsonb,
     '["api.machines.dev","api.upstash.com","slack.com","api.github.com"]'::jsonb, 'public',
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
     (extract(epoch from now()) * 1000)::bigint)
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    source_repo = EXCLUDED.source_repo,
    source_ref = EXCLUDED.source_ref,
    required_credentials = EXCLUDED.required_credentials,
    required_credentials_reasons = EXCLUDED.required_credentials_reasons,
    network_hosts = EXCLUDED.network_hosts,
    updated_at = EXCLUDED.updated_at
 WHERE core.fleet_library.content_hash IS NULL;
