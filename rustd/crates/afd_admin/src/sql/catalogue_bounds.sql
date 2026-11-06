SELECT set_config('lock_timeout', $1, true), set_config('statement_timeout', $2, true)
