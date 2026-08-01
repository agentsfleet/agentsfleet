-- Substrate, part one: the domain schemas every later slot creates tables in.
--
-- Layer 1xx is the only layer that runs before any table exists. Schema creation
-- is split from the role and privilege baseline (slot 110) because the two are
-- separate concerns with separate failure modes: a missing schema fails the very
-- next slot loudly, while a missing grant fails at runtime under a specific
-- role. Keeping them apart also keeps each file inside the single-concern bound.

CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS fleet;
CREATE SCHEMA IF NOT EXISTS billing;
CREATE SCHEMA IF NOT EXISTS vault;
CREATE SCHEMA IF NOT EXISTS memory;

-- Migration bookkeeping and the immutable operator audit trail. The tables
-- themselves are created by the migration runner (db/pool_migrations.zig) before
-- any slot executes, so this slot declares only the schema.
CREATE SCHEMA IF NOT EXISTS audit;

-- The `ops_ro` schema is deliberately absent. It was created, granted, and
-- placed on two roles' search_path while holding zero tables for its whole life.
-- Read-only principals are routed to `audit` alone until a slot actually creates
-- something for them to read: no schema is created that holds no tables.
