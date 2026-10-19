-- The nine owned schemas, one per product area (docs/architecture/postgres_schemas.md §Layout).
--
-- Schema creation runs before any table exists and is split from the role baseline
-- (slot 1010): a missing schema fails the next slot loudly, a missing grant fails at
-- runtime under one role, so the two failures stay apart.
--
-- `audit` is created by the migrator itself (`create_schema` in `afd_db::migrate`),
-- because SQLx writes its ledger there before this slot runs.

CREATE SCHEMA identity;
CREATE SCHEMA authoring;
CREATE SCHEMA fleet;
CREATE SCHEMA runner;
CREATE SCHEMA connector;
CREATE SCHEMA model;
CREATE SCHEMA billing;
CREATE SCHEMA vault;
CREATE SCHEMA memory;
