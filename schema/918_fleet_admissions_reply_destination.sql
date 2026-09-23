-- Where an admission's answer goes, recorded by the producer that owns a reply
-- surface.
--
-- An answer used to be owed to `core.runner_leases.provider`, which is the
-- MODEL provider resolved at billing, so every non-empty answer from every
-- producer became a delivery no connector could take, re-offered forever. The
-- destination belongs to where the question arrived: a Slack mention knows its
-- thread, a steer and a cron fire have no reply surface at all. So the producer
-- records it here, at admission, and the report owes a delivery only when the
-- row it settles carries one.
--
-- `reply_provider` is a connector id and `reply_address` an opaque address only
-- that connector's poster reads. Both NULL for every producer that owns no
-- reply surface — the steer, both webhooks, the schedule fire and the repair
-- verification — and a gate continuation copies the pair of the event it
-- resumes, inside its own INSERT (`afd_admission::sql::INSERT_ADMISSION`).
--
-- On `core.fleet_admissions` and not `core.fleet_events`: the report finds this
-- row by the logical event id the delivery stamp already uses, and a copy on a
-- second table would be a second row that could disagree.
--
-- Both-or-neither is a pair of NULL tests, not a value list, so no connector id
-- the application names is spelled here (RULE STS); which ids exist is
-- `afd_connector::Provider`'s to say, and the report parses the stored one.
--
-- Additive: two nullable columns take no rewrite, and the check holds for every
-- existing row, whose pair is NULL. RULE SGR does not apply: no object is
-- created, and access runs through the table grants in
-- schema/910_fleet_admissions.sql.
ALTER TABLE core.fleet_admissions
    ADD COLUMN IF NOT EXISTS reply_provider TEXT,
    ADD COLUMN IF NOT EXISTS reply_address  TEXT;

-- PostgreSQL has no `ADD CONSTRAINT IF NOT EXISTS`, so the slot drops the name
-- it authors first, as schema/916_usage_ledger_fleet_scoped_key.sql does.
ALTER TABLE core.fleet_admissions
    DROP CONSTRAINT IF EXISTS ck_fleet_admissions_reply_both_or_neither;

ALTER TABLE core.fleet_admissions
    ADD CONSTRAINT ck_fleet_admissions_reply_both_or_neither
    CHECK ((reply_provider IS NULL) = (reply_address IS NULL));

-- The lookup the report and a continuation make: the destination of one event,
-- by the logical id's two integers. The event a report settles was DELIVERED,
-- so neither partial index on `delivered_at IS NULL` (slots 910, 914) covers
-- it, and without this the read falls to `idx_fleet_admissions_fleet_id` and
-- walks the fleet's whole history once per lease — the cost slot 914 measured.
-- Partial on the destination itself: only rows that owe an answer enter, the
-- index is empty the day this slot applies, and a row with no destination is
-- simply not found, which is the answer the report wants for it. A NULL test,
-- not a value literal (RULE STS). Built inside the slot's transaction for the
-- reason slot 914 gives; with no row qualifying yet, the build writes nothing.
CREATE INDEX IF NOT EXISTS idx_fleet_admissions_reply_lookup
    ON core.fleet_admissions (fleet_id, created_at, seq)
    WHERE reply_provider IS NOT NULL;
