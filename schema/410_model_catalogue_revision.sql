-- Catalogue generation counter — the one thing the response and billing caches
-- both agree on.
--
-- The global model catalogue is read on a hot path and cached in-process, but it
-- is also admin-mutable. Without a generation, a replica can serve a page built
-- from one catalogue state while billing prices the same request from another —
-- the two caches drift independently and nothing detects it, because each is
-- internally consistent.
--
-- This row is that generation. Every request reads it after authentication and
-- BEFORE selecting a cache entry; the revision then forms part of the response
-- cache key, so a candidate built from revision N lands under a key containing N
-- and a request that has read N+1 simply looks somewhere else. That is what
-- makes a stale candidate unreachable rather than dangerous, and it is why no
-- publish-ordering protocol is needed on top (see state/model_library_cache.zig).
--
-- A mutation locks THIS row FOR UPDATE, changes the catalogue, and increments
-- the revision in the same transaction. The lock is what serializes concurrent
-- admin mutations: two of them cannot both read revision N and both write N+1.
--
-- Identity exception (SCHEMA_CONVENTIONS "Identity Column"): a singleton keyed
-- by a pinned integer carries no UUID. `id` is constrained to a single numeric
-- value rather than left to convention, so a second generation row cannot be
-- inserted by a future writer that did not know the table was meant to hold one.
-- The CHECK is numeric — RULE STS bans string-literal value sets, and an
-- integer identity guard is not one.
--
-- `revision` is BIGINT and only ever increases. It is not a timestamp: two
-- mutations inside the same millisecond must still produce two generations, and
-- a clock adjustment must never move a generation backwards.

CREATE TABLE IF NOT EXISTS core.model_catalogue_revision (
    id          SMALLINT PRIMARY KEY,
    revision    BIGINT   NOT NULL,
    updated_at  BIGINT   NOT NULL,
    CONSTRAINT ck_model_catalogue_revision_singleton CHECK (id = 1)
);

-- Seed the singleton so every reader finds a row. A revision read returning no
-- row is indistinguishable from a failed read, and the endpoint answers
-- UZ-LIBRARY-004 (503) for a failed read — so an unseeded table would take the
-- catalogue offline rather than start it at generation zero.
INSERT INTO core.model_catalogue_revision (id, revision, updated_at)
VALUES (1, 0, 0)
ON CONFLICT (id) DO NOTHING;

-- No `created_at`: the row is seeded by this slot at bootstrap and never
-- created again, so a birth instant would record when the migration ran and
-- nothing would ever read it.

-- api_runtime reads the revision on every catalogue request and increments it on
-- an admin mutation. No INSERT: the singleton is seeded here and must never gain
-- a second row. No DELETE: removing it would take the catalogue offline.
GRANT SELECT, UPDATE ON core.model_catalogue_revision TO api_runtime;

-- No index. The table holds exactly one row, so the primary key is the whole
-- access path and any additional index would be dead weight the planner still
-- has to maintain.
