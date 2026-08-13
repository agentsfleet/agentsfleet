-- Add the exact commit GitHub reports for a merged repair Pull Request. Slot
-- 830 stays intact; its deploy-stamp update remains legal while older daemon
-- instances drain during rolling replacement. New code does not read or write
-- that pair.

ALTER TABLE core.repair_pr_links
    ADD COLUMN IF NOT EXISTS merged_commit_sha TEXT;

ALTER TABLE core.repair_pr_links
    ADD COLUMN IF NOT EXISTS merged_at BIGINT;

CREATE INDEX IF NOT EXISTS idx_repair_pr_links_workspace_repository_merged_commit
    ON core.repair_pr_links (workspace_id, lower(repository), merged_commit_sha)
    WHERE merged_commit_sha IS NOT NULL;

CREATE OR REPLACE FUNCTION core.repair_pr_links_content_frozen() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF current_setting('fleet.allow_gate_purge', true) = 'on' THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'repair_pr_links is append-only -- DELETE is not permitted';
    END IF;
    IF NEW.id IS DISTINCT FROM OLD.id
        OR NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
        OR NEW.fleet_id IS DISTINCT FROM OLD.fleet_id
        OR NEW.event_id IS DISTINCT FROM OLD.event_id
        OR NEW.repository IS DISTINCT FROM OLD.repository
        OR NEW.branch IS DISTINCT FROM OLD.branch
        OR NEW.pr_number IS DISTINCT FROM OLD.pr_number
        OR NEW.pr_url IS DISTINCT FROM OLD.pr_url
        OR NEW.created_at IS DISTINCT FROM OLD.created_at
    THEN
        RAISE EXCEPTION 'repair_pr_links content is immutable';
    END IF;

    -- Preserve the slot-830 update used by an older daemon after migrations
    -- run but before every instance has been replaced. Merge identity must not
    -- change in the same statement.
    IF NEW.merged_commit_sha IS NOT DISTINCT FROM OLD.merged_commit_sha
        AND NEW.merged_at IS NOT DISTINCT FROM OLD.merged_at
    THEN
        RETURN NEW;
    END IF;
    IF NEW.deploy_status IS DISTINCT FROM OLD.deploy_status
        OR NEW.deploy_stamped_at IS DISTINCT FROM OLD.deploy_stamped_at
    THEN
        RAISE EXCEPTION 'repair_pr_links merge identity and deploy stamp change separately';
    END IF;
    IF OLD.merged_commit_sha IS NOT NULL
        OR OLD.merged_at IS NOT NULL
        OR NEW.merged_commit_sha IS NULL
        OR NEW.merged_at IS NULL
    THEN
        RAISE EXCEPTION 'repair_pr_links merge identity changes at most once';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
