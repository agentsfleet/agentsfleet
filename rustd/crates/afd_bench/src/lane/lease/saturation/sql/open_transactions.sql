SELECT (EXTRACT(EPOCH FROM clock_timestamp() - xact_start) * 1000)::float8
  FROM pg_stat_activity
 WHERE datname = current_database()
   AND backend_type = 'client backend'
   AND xact_start IS NOT NULL
   AND pid <> pg_backend_pid()
