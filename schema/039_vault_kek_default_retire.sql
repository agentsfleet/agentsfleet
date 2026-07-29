-- 039: retire the v1 envelope entirely.
--
-- Every vault envelope is AAD-bound (kek_version 2) — the pre-binding v1 format
-- (`0ff4902ca`, Jul 11 2026) shipped before any secret this deployment holds,
-- so no v1 row has ever existed here. Pre-production, v1 is removed outright
-- rather than tolerated:
--
--   1. DROP the DEFAULT that still named version 1. Every writer binds the
--      version explicitly, so the default never fired on a correct write; the
--      only row it could mint is one a forgotten column would silently create.
--   2. CHECK that kek_version is the one current version. This makes a v1 (or
--      any non-current) row structurally impossible — a mistaken write fails
--      loudly at the database instead of landing a row nothing can decrypt.
--      The AEAD tag is the ultimate guard (a v1 ciphertext read under the bound
--      AAD fails authentication), so the check is the single explicit assertion
--      of the invariant, not a second-guess of it.
--
-- Both statements are idempotent: DROP DEFAULT on a column with none is a no-op,
-- and the constraint add swallows a duplicate. Every existing row is version 2,
-- so the check validates without a rewrite. A future envelope format changes
-- this constraint in its own migration, alongside the code that reads it.
ALTER TABLE vault.secrets ALTER COLUMN kek_version DROP DEFAULT;

DO $$ BEGIN
  ALTER TABLE vault.secrets
    ADD CONSTRAINT ck_vault_secrets_kek_version_current CHECK (kek_version = 2);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
