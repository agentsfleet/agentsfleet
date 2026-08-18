-- The operator's extra sandbox binds, delivered to the host on its heartbeat.
--
-- Why: a path missing from the daemon-owned baseline should be repairable from
-- the dashboard rather than by a deploy. The runner already consumes this list
-- (`sandbox_args.composeBinds`) and the API already validates it
-- (`extraBindsValid`); without this column the assignment was accepted, then
-- dropped on the floor, and every heartbeat delivered an empty list.
--
-- Lives on `fleet.runners` beside the rest of the assignment (`sandbox_tier`,
-- `network_policy`, `registry_allowlist`, `worker_count`). Every read that
-- already selects those four carries this along for free rather than gaining a
-- join, and the assignment stays one row that cannot be read torn.

-- The ordered list, each entry `{path, mode, note}`. NULL rather than a
-- defaulted empty array: every runner enrolled before this column exists reads
-- NULL, and the decoder resolves that to "baseline only" — the same answer the
-- wire gives for an omitted field, so an older row and an older control plane
-- agree. Not fail-closed, because an absent extra list is the NORMAL state; a
-- runner with no operator additions must keep leasing.
--
-- No CHECK constraint and no DEFAULT: the mode vocabulary (`read_only` /
-- `read_write`) lives in `protocol_bind.BindMode`, and RULE STS keeps those
-- values out of schema objects so they cannot drift from the constants that own
-- them. `extraBindsValid` is the enforcement, run on both sides of the wire.
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS extra_binds JSONB;

-- No index and no new grants. Every access path reaches this column through
-- `fleet.runners.id`, which is the primary key, and `schema/600` already grants
-- api_runtime SELECT/INSERT/UPDATE/DELETE on the table — column privileges are
-- inherited, so a table-level grant already covers columns added later.
