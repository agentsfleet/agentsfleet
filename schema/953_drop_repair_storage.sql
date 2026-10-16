-- The production-repair evidence store, retired with the dispatcher it fed.
--
-- `core.repair_pr_links` (slots 830, 832), `core.repair_run_results` (831),
-- `core.repair_production_results` (834) and `core.repair_verifications` (835,
-- 912) held repair linkage, run history, production results and verification
-- intents. Nothing writes the first three, and the verification dispatcher
-- that read the last is gone, so all four go with their guard functions.
-- `idx_fleets_workspace_status_id`, which slot 835 created on `core.fleets`,
-- is not this storage's and stays.
--
-- Owner decision, Indy, Oct 15, 2026. VERSION is 0.61.0, above the
-- 0.30.0 anchor, so slots 830 to 835 and 912 stay frozen history.
--
-- Dropped referrer first: verifications point at links and production
-- results. Each table takes its triggers with it; the functions follow.

DROP TABLE IF EXISTS core.repair_verifications;
DROP TABLE IF EXISTS core.repair_production_results;
DROP TABLE IF EXISTS core.repair_run_results;
DROP TABLE IF EXISTS core.repair_pr_links;

DROP FUNCTION IF EXISTS core.repair_verifications_fenced_update();
DROP FUNCTION IF EXISTS core.repair_production_results_append_only();
DROP FUNCTION IF EXISTS core.repair_run_results_frozen();
DROP FUNCTION IF EXISTS core.repair_pr_links_content_frozen();
