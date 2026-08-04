-- The tenant: the billing and ownership root every other row resolves to.
--
-- Identity is one column. `id` is the primary key, application-minted UUID
-- version 7 (id_format.generateUuidV7), and nothing else in the table carries
-- the same value. The pre-rebuild shape paired a generated identity primary key
-- with a `tenant_id UUID NOT NULL UNIQUE` twin holding an identical value,
-- which cost two btree indexes over the same sixteen bytes and — the reason
-- that actually matters — left the table with two unique keys. `ON CONFLICT`
-- arbitrates exactly one constraint, so two sessions inserting a brand-new row
-- race to a duplicate-key error on whichever index the statement did not name,
-- instead of taking the update arm. `schema/043` recorded that per-table before
-- it became the convention.
--
-- The uuidv7 CHECK pins the version nibble only, and is a smoke alarm rather
-- than the authority: `id` is server-minted by the sole generator, whose unit
-- tests pin the variant bits and the canonical lowercase form. A full-shape
-- regular expression was measured at 17x this check's per-row cost inline, and
-- 56x through a shared IMMUTABLE function (a SET search_path clause blocks
-- inlining), which is a permanent tax to re-catch what the generator's tests
-- already catch.

CREATE TABLE IF NOT EXISTS core.tenants (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_tenants_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    name        TEXT NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);

-- api_runtime owns tenant lifecycle through signup bootstrap and the account
-- erasure path (the Clerk user.deleted webhook).
GRANT SELECT, INSERT, UPDATE, DELETE ON core.tenants TO api_runtime;
