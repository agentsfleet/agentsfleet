-- The two DELETE grants the hard purge has always needed and never held.
--
-- `Fleets::purge` deletes a killed fleet's child rows before the fleet row, and
-- two of those deletes were never granted: schema/510 and schema/810 each give
-- `api_runtime` SELECT, INSERT and UPDATE on their table and stop there, while
-- `afd_fleet_lifecycle`'s `PURGE_CHILDREN` issues a DELETE against both. RULE
-- SGR asks a migration to grant every privilege its table's callers exercise;
-- these two slots granted the three they could see a handler using, and the
-- purge arrived later.
--
-- Why a live database never reported it: the PlanetScale login role the daemon
-- authenticates as held `pg_write_all_data`, a built-in role carrying INSERT,
-- UPDATE and DELETE on every table plus USAGE on every schema. Nothing the
-- daemon did could be refused, so the missing grants were unobservable —
-- `has_table_privilege('api_runtime', …, 'DELETE')` answered false the whole
-- time and no request ever asked. A database branch created without that
-- membership refuses `fleet delete` on its first attempt, which is how this was
-- finally found.
--
-- Deliberately not solved by re-granting `pg_write_all_data`: that role also
-- opens `vault.secrets` and `billing.tenant_wallet`, which schema/110 fences on
-- purpose. Two table-scoped grants restore the purge and nothing else.
--
-- No grant for `memory.memory_entries` here, and that is not an omission. Memory
-- sits behind `memory_runtime`, which `api_runtime` holds WITH INHERIT FALSE and
-- must assume per transaction (schema/110). The purge now assumes it, the way
-- every other memory reader already did.

GRANT DELETE ON core.fleet_approval_gates TO api_runtime;
GRANT DELETE ON core.fleet_sessions TO api_runtime;
