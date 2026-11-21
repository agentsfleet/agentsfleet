-- Holds one run's call until this transaction ends, so a retry of the same call waits for
-- the first to write its item. Transaction-scoped: a rollback or a dropped connection lets
-- it go, and a hash collision only makes two calls take turns.
--
-- $1 run · $2 the call's number in it
SELECT pg_advisory_xact_lock(uuid_hash($1), hashint8($2))
