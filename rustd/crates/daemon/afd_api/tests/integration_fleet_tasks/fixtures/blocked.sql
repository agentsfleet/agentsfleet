SELECT count(*)::bigint
FROM pg_stat_activity
WHERE datname = current_database() AND usename = current_user
  AND $1::integer = ANY(pg_blocking_pids(pid))
