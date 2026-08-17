-- A runner carries its own connectivity verdict, produced INSIDE the sandbox a
-- lease runs in.
--
-- Why: a runner whose sandbox cannot resolve a hostname reports itself healthy.
-- Every check `agentsfleet-runner doctor` runs executes on the HOST, outside the
-- unshared mount namespace a lease actually runs in, so a dangling
-- `/etc/resolv.conf` inside the sandbox is invisible to it — the runner reads
-- ACTIVE·ONLINE while every lease dies on name resolution.
--
-- Columns on `fleet.runners`, not a child table. The self-test is the same arc
-- as the capability report that already lives here — a host describing itself,
-- reported upward on the heartbeat — with an executed proof in place of a static
-- probe. That arc's write is ONE statement
-- (`UPDATE_RUNNER_CAPABILITY_AND_VERDICT`), and these columns join it rather than
-- adding a second write to every reporting heartbeat; both runner reads already
-- select `capability_report` and carry these along for free instead of gaining a
-- join. A keyed child table would have bought a separate row lifecycle nothing
-- here needs.

-- The operator's outstanding ask. `requested_at IS NOT NULL` IS the pending
-- state — no status vocabulary, which RULE STS keeps out of schema objects so it
-- cannot drift from the application constants that own it. Cleared by the daemon
-- when it reports the matching verdict.
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS selftest_requested_at BIGINT;

-- When the verdict landed. NULL until a first report: a runner may hold a
-- request with no result (never yet reported), or a result with no request (the
-- startup probe, which no operator asked for).
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS selftest_completed_at BIGINT;

-- The ordered per-check verdicts, each `{name, ok, detail}`. Left NULL rather
-- than defaulted to an empty array: NULL means "never self-tested", which the
-- page renders differently from "tested and reported no checks". Mirrors
-- `capability_report`, which is likewise nullable on a runner that has not yet
-- reported one.
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS selftest_checks JSONB;

-- Whether every check passed, decided by the daemon that ran them. Stored rather
-- than derived so the runner list filters on it without opening the JSONB per
-- row.
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS selftest_all_ok BOOLEAN;

-- The assignment the probe RAN UNDER, not the one in force now. A result
-- outlives the policy that produced it: re-assigning a runner to
-- `deny_all_egress` does not re-run its self-test, and rendering the old verdict
-- as current would tell an operator their new policy is proven when nothing has
-- tested it. The read compares these two against the live `sandbox_tier` and
-- `network_policy` on this same row and labels a mismatch stale (Dimension 1.3);
-- same-row means that comparison needs no join and cannot read a torn pair.
--
-- Plain TEXT, no CHECK constraint: the tier and policy vocabularies live in
-- application constants and RULE STS keeps them out of schema objects.
ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS selftest_sandbox_tier TEXT;

ALTER TABLE fleet.runners
    ADD COLUMN IF NOT EXISTS selftest_network_policy TEXT;

-- No index and no new grants. Every access path reaches these columns through
-- `fleet.runners.id`, which is the primary key, and `schema/600` already grants
-- api_runtime SELECT/INSERT/UPDATE/DELETE on the table — column privileges are
-- inherited, so a table-level grant already covers columns added later.
