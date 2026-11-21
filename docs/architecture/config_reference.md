# `agentsfleetd` configuration reference

`agentsfleetd` reads one TOML file, named by `AGENTSFLEET_CONFIG_FILE`. Every
key is optional, and each default below is the value the daemon used before
the key existed. The parser lives in `rustd/crates/daemon/main/src/config/`
(`settings.rs` for the file's shape, `tuning.rs` for the sections below).

## Boot checks

Startup refuses the file, with `UZ-STARTUP-004`, when:

- a count, size or timeout is `0`;
- a default exceeds its maximum: `default_page_size` over `max_page_size`,
  `default_thread_page_size` over `max_thread_page_size`,
  `default_wake_queue_size` or `wake_queue_capacity_fleets` over
  `max_wake_queue_size`, `min_connections` over `max_connections`;
- `small_model_max_tokens` is not below `large_model_min_tokens`;
- `drain_timeout_seconds` is under 6, the time a runner answer already in
  flight needs, or `task_join_timeout_seconds` is under 9, the time the
  telemetry flush needs;
- `max_message_mib` or `max_control_message_mib` exceeds the frame the runner
  link carries (32 MiB and 2 MiB);
- an `[auth]` value would weaken sign-in: a lifetime or grace over one day,
  an identity-provider call over 60 seconds, more than 10 wrong login codes,
  or `installation_token_ttl_minutes` over GitHub's 60.

## Precedence

`[database]` holds defaults. `DATABASE_POOL_SIZE`, `DATABASE_MIN_POOL_SIZE`,
`DATABASE_ACQUIRE_TIMEOUT_MS` and their `_API` and `_MIGRATOR` forms override
them, and `[database.api]` / `[database.migrator]` override both. When the
environment lowers the pool below `min_connections`, the warm floor follows it. With `min_connections` unset anywhere, a quarter of the resolved ceiling is
kept warm: 5 of the default 20.

`[nats] wake_queue_capacity_fleets` and `AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY`
choose the queue's size; `default_wake_queue_size` applies when neither does.

## Sections

The file below adds sections to the keys the file already accepted
(`[server] port`, `[runner.*]`, `[outbound.retries]`, `[telemetry.otlp]` and
the others in `settings.rs`).

```toml
[server]                                    # the HTTP API
max_in_flight_requests     = 256            # requests served at once; past this the API answers 503 so it never falls over
# read_timeout_seconds       = 300            # how long one request may take to arrive before it is dropped  (not read yet; see below)
drain_timeout_seconds      = 15             # on shutdown, how long open requests get to finish
task_join_timeout_seconds  = 10             # on shutdown, how long background tasks get to stop
max_live_activity_notices  = 16             # live-activity updates sent to dashboards at the same time

[api]                                       # list reads
default_page_size          = 50             # rows a list returns when the caller names no size
max_page_size              = 100            # most rows any list returns at once
default_thread_page_size   = 25             # messages a fleet's chat returns when the caller names no size
max_thread_page_size       = 100            # most messages a fleet's chat returns at once

[database]                                  # the Postgres connection pool
max_connections            = 20             # connections this daemon may hold; keep the sum across replicas under Postgres max_connections
min_connections            = 5              # connections kept open so the first requests after idle are fast
acquire_timeout_seconds    = 5              # how long a request waits for a free connection before failing
connect_timeout_seconds    = 10             # how long opening a new connection may take
idle_timeout_minutes       = 10             # an unused connection is closed after this
max_connection_lifetime_minutes = 30        # every connection is replaced after this, so a failover is picked up

[nats]                                      # the message bus
default_wake_queue_size    = 64             # wake-ups held per daemon before older ones are dropped (work is never lost: it stays in Postgres)
max_wake_queue_size        = 65_536         # the most the wake queue may be raised to

[runners]                                   # the link between the daemon and runners
# default_workers_per_runner = 1              # runs a new runner executes at once; change per runner in the dashboard  (not read yet; see below)
# heartbeat_interval_seconds = 10             # how often a runner checks in; sent to runners when they connect  (not read yet; see below)
max_message_mib            = 32             # largest message on the runner link; sent to runners when they connect
max_control_message_mib    = 2              # largest control message on the runner link; sent to runners when they connect
assign_batch_size          = 64             # waiting turns considered per assignment pass
max_repair_backoff_seconds = 5              # longest wait before retrying a broken runner connection repair

[turns]                                     # queued and running turns
max_runs_per_turn          = 3              # a turn whose runner dies is retried on another; after this many runs it fails
max_queued_turns           = 100_000        # queued turns across the deployment; past this new messages are refused
history_page_size          = 100            # earlier turns read per page when a run is given its chat history

[turns.checkpoints]                         # turn checkpoints stored in R2
retention_days             = 7              # a settled turn's checkpoint is deleted after this
upload_url_ttl_minutes     = 15             # how long a runner's upload link stays valid
restore_pin_minutes        = 2              # a checkpoint being restored cannot be deleted during this window
orphan_after_minutes       = 60             # an upload never committed is deletable after this

[context]                                   # how much tool history a run keeps in its context
small_model_max_tokens     = 200_000        # a model with a window at or below this uses the small tool window
large_model_min_tokens     = 1_000_000      # a model with a window at or above this uses the large tool window
small_tool_window_count    = 10             # recent tool calls kept for a small model
default_tool_window_count  = 20             # recent tool calls kept for a model in between
large_tool_window_count    = 30             # recent tool calls kept for a large model

[fleets]                                    # fleet definitions
definition_cache_mib       = 64             # memory for parsed fleet definitions, so a run does not re-parse them
head_cache_count           = 16_384         # fleets whose latest revision is kept in memory
# name_generation_attempts   = 3              # tries at a free generated fleet name before asking for one  (not read yet; see below)

[billing]                                   # money
# max_daily_fleet_budget_usd   = 1_000        # the highest daily budget a fleet may be given  (not read yet; see below)
# max_monthly_fleet_budget_usd = 10_000       # the highest monthly budget a fleet may be given  (not read yet; see below)
price_book_cache_count     = 1_024          # model prices kept in memory for metering runs

[tasks]                                     # durable tasks
slice_seconds              = 600            # a task runs in slices of this length; its funds are checked before each
operation_timeout_seconds  = 30             # how long one task operation (GitHub read, storage call) may take
terminal_retention_days    = 7              # a finished task's records are kept this long

[tasks.checkpoints]                         # task checkpoints stored in R2
keep_generations           = 3              # newest checkpoints kept per task; older ones are deleted at commit
upload_url_ttl_minutes     = 15             # how long a runner's upload link stays valid
download_url_ttl_minutes   = 5              # how long a runner's download link stays valid
restore_pin_minutes        = 30             # a checkpoint being restored cannot be deleted during this window
orphan_after_minutes       = 60             # an upload never committed is deletable after this
delete_batch_size          = 256            # objects deleted per storage call

[setup]                                     # the setup chat
budget_seconds             = 15             # how long one setup request may take before it is answered as busy
replay_window_hours        = 24             # a repeated setup request within this returns the first answer
scan_batch_size            = 64             # drafts read per page by setup maintenance

[auth]                                      # sign-in, sessions and tokens (startup refuses unsafe values)
session_ttl_seconds        = 300            # how long a CLI login session waits to be approved
session_replay_seconds     = 60             # a login code reused within this is refused
max_session_verify_attempts = 5             # wrong codes allowed before a login session is closed
access_token_ttl_minutes   = 5              # how long a minted access token lives
oauth_state_ttl_minutes    = 10             # how long a connector sign-in link stays valid
identity_timeout_seconds   = 5              # how long a call to the identity provider may take
jwks_fetch_timeout_seconds = 5              # how long fetching the identity provider's signing keys may take
jwks_cache_hours           = 6              # how long signing keys are trusted before refetching
jwks_stale_grace_minutes   = 15             # keys still accepted this long after a failed refetch
jwks_min_refresh_seconds   = 30             # signing keys are refetched no more often than this

[credentials]                               # provider and connector credentials held by the daemon
token_cache_count          = 8_192          # minted tokens kept in memory
# token_cache_minutes        = 5              # how long a cached token is reused  (not read yet; see below)
platform_key_cache_minutes = 5              # how long a platform model key is reused before re-reading the vault

[connectors.github]
installation_token_ttl_minutes = 60         # how long a GitHub app token is used; GitHub's own maximum is 60

[connectors.slack]
post_timeout_seconds         = 5            # how long posting an answer to Slack may take
answer_check_timeout_seconds = 3            # how long checking whether a thread was already answered may take
read_timeout_ms              = 1_500        # how long reading a Slack thread may take
# signature_clock_skew_seconds = 300          # a Slack request older than this is refused as a replay  (not read yet; see below)

[connectors.mail]
send_timeout_seconds         = 10           # how long sending one email may take

[connectors.qstash]                         # fleet schedules
attempt_timeout_ms           = 2_500        # how long one call to QStash may take
call_budget_seconds          = 10           # total time for a QStash call, retries included
max_attempts                 = 3            # tries per QStash call
max_retry_delay_ms           = 800          # longest pause between tries

[telemetry]                                 # metrics and traces
# collect_interval_seconds     = 60           # how often metrics are exported  (not read yet; see below)
# span_batch_size              = 512          # traces sent per export  (not read yet; see below)
max_metric_series            = 4_096        # distinct metric series kept; past this new ones are dropped
metric_max_age_seconds       = 120          # a metric not updated within this is dropped
```

The commented-out keys are refused if set, because the daemon does not read them yet:

- `[runners] heartbeat_interval_seconds` repeats
  `[runner.health] heartbeat_interval_seconds`, which already sets the interval
  and is already sent to a runner when it connects.
- `[telemetry] collect_interval_seconds` and `span_batch_size` repeat
  `[telemetry.otlp] metrics_export_interval_seconds` and `trace_batch_size`.
- `[credentials] token_cache_minutes`: the token cache keeps each token until
  its own expiry, so no five-minute setting exists to replace.
- `[server] read_timeout_seconds`: the HTTP server sets no request read
  timeout; the only 300-second read timeout is the outbound storage client's.
- `[runners] default_workers_per_runner`: a runner names its own worker count
  when it registers, and no daemon code picks one for it.
- `[fleets] name_generation_attempts`: the only three-try name generator names
  workspaces, not fleets.
- `[connectors.slack] signature_clock_skew_seconds`: the signed-request replay
  window is one 300-second window shared by GitHub, Svix and Slack, so a
  Slack-only key would split it.
- `[billing] max_daily_fleet_budget_usd` and `max_monthly_fleet_budget_usd`:
  the caps are checked inside the fleet definition parser the runner shares, and
  a lowered cap would also refuse definitions already saved.

## The runner link

`max_message_mib` and `max_control_message_mib` are sent to a runner in its
welcome as `max_message_bytes` and `max_control_message_bytes`; a welcome
without them means 32 MiB and 2 MiB. The runner does not act on them yet, and
both sides still frame against the 32 MiB and 2 MiB the protocol compiles in,
so these keys can only announce a value at or below those.
