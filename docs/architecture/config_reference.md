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
transfer_window_minutes    = 15             # how long a checkpoint's uploads or restore may run in total
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
upload_link_ttl_minutes    = 5              # how long a runner's presigned upload link stays valid
download_link_ttl_minutes  = 5              # how long a runner's presigned download link stays valid
transfer_window_minutes    = 15             # how long a checkpoint's uploads or restore may run in total
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
assumed_token_ttl_minutes  = 5              # used when a provider's token states no expiry

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
without them means 32 MiB and 2 MiB. The runner frames that connection to them
(see the runner's section below); the daemon still frames against the 32 MiB
and 2 MiB the protocol compiles in, so these keys can only announce a value at
or below those.

# `agentsfleet-runner` configuration reference

`agentsfleet-runner` reads one TOML file, named by
`AGENTSFLEET_RUNNER_CONFIG_FILE`; a blank or unset variable means every
default. Every key is optional, and each default below is the value the
runner used before the key existed. The parser lives in
`rustd/crates/runner/main/src/config/` (`mod.rs` for the file's shape,
`tuning.rs` for the sections below). The file already accepted
`[inference]`, `[provider_retries]` and `[supervisor]`; they are unchanged.

## Boot checks

`run` refuses the file, with `UZ-STARTUP-004`, before it exports or dials the
daemon, when:

- a count, size or timeout is `0`;
- `min_exec_yield_ms` exceeds `max_exec_yield_ms`, or `[egress]
  connect_timeout_seconds` exceeds `request_timeout_seconds`;
- `default_shell_timeout_seconds` is over 29: with the executor's 5-second
  ending grace it must stay under the run's 35-second lease;
- `max_sessions_per_run` is 8 or fewer: the 8 most recently used sessions are
  never closed to make room, so a smaller cap would not hold;
- `max_subagent_depth` is over 2, the depth a restore checks a checkpoint's
  children against;
- `[daemon] call_timeout_seconds` is 12 or less, the daemon's own message
  deadline, or `max_activity_batches_held` is under 2 (one in flight, one
  queued);
- a key below is commented out: it is refused as unknown.

## Sections

```toml
[sandbox]                                   # the box each run executes in
memory_mib                   = 2_048        # memory one run may use
disk_mib                     = 4_096        # disk one run may use
io_mib_per_second            = 200          # disk read/write speed one run may use
ready_timeout_seconds        = 10           # how long a new sandbox may take to start
# server_grace_seconds         = 5            # how long the in-sandbox server gets to stop  (not read; see below)
# far_connect_timeout_seconds  = 3            # how long connecting into the sandbox may take  (not read; see below)
# max_attach_attempts          = 7            # tries to attach to a starting sandbox  (not read; see below)
# min_retry_delay_ms           = 100          # first pause between attach tries  (not read; see below)
# max_retry_delay_seconds      = 30           # longest pause between attach tries  (not read; see below)

[tools]                                     # what a run's tools may do
default_shell_timeout_seconds = 10          # how long a shell command runs when the model names no timeout
min_exec_yield_ms            = 250          # shortest wait for a running command's output
max_exec_yield_ms            = 30_000       # longest wait for a running command's output
max_sessions_per_run         = 64           # interactive shell sessions one run may keep open
# output_edge_kib              = 512          # command output kept from the start and end  (not read; see below)
# max_read_mib                 = 8            # largest file a read returns  (not read; see below)
default_read_lines           = 2_000        # lines a read returns when the model names no limit
max_image_dimension_px       = 2_048        # larger images are scaled down to this before the model sees them
max_image_mib                = 5            # largest image sent to the model after scaling
max_sleep_hours              = 12           # longest a sleep may wait
# mcp_call_timeout_seconds     = 300          # how long one MCP tool call may take  (not read; see below)
max_wait_seconds             = 3_600        # longest a run waits for a subagent
max_open_subagents           = 6            # subagents open at once in one run
max_subagent_depth           = 2            # how deep subagents may start their own subagents

[provider]                                  # decision calls and token counts
connect_timeout_seconds      = 3            # how long connecting to a provider for a decision call may take
physical_timeout_seconds     = 12           # how long one decision call may take, its whole reply included
count_timeout_seconds        = 10           # how long a token-count call may take
max_count_reply_kib          = 16           # largest token-count reply accepted

[egress]                                    # outbound HTTP from tools (web fetch, connectors)
connect_timeout_seconds      = 10           # how long connecting may take
request_timeout_seconds      = 30           # how long one request may take; a GET's redirect chain shares it
# max_response_mib             = 1            # largest response a tool reads  (not read; see below)
# max_construction_mib         = 10           # largest request a tool may build  (not read; see below)
# max_target_kib               = 8            # longest URL a tool may call  (not read; see below)
max_redirects                = 3            # redirects followed before giving up

[daemon]                                    # this runner's link to agentsfleetd
call_timeout_seconds         = 20           # how long one call to the daemon may take
# max_report_mib               = 1            # largest run report sent at once  (not read; see below)
# max_activity_kib             = 128          # largest live-activity update sent at once  (not read; see below)
max_activity_batches_held    = 4            # activity updates held while the daemon is busy
max_activity_batch_kib       = 64           # largest held activity batch

# [executor]                                # the process that runs commands inside the sandbox
# max_frame_mib                = 64           # largest message between the runner and the executor  (not read; see below)

[telemetry]
max_spans_per_run            = 256          # traces kept per run
```

`[provider]` bounds decision calls (`af_providers::decision`) and native token
counts; the model stream's own connect and silence bounds stay
`[inference] connect_timeout_seconds` and `stream_idle_timeout_seconds`.

The commented-out keys are refused if set, because no runner code reads them:

- `[sandbox] server_grace_seconds`: the only 5-second server grace is the
  unsandboxed engine's, which a release build refuses to construct.
- `[sandbox] far_connect_timeout_seconds`: the 3-second timeout belongs to the
  kernel lane's `test-util` far host, not to a running sandbox.
- `[sandbox] max_attach_attempts`: the 7 is how many more free loop devices
  are tried after the first when another process takes the one offered (8
  tries in all), not attaches to a sandbox.
- `[sandbox] min_retry_delay_ms` and `max_retry_delay_seconds`: loop-device
  tries do not pause; the 100 ms to 30 s backoff is the warm-slot pool's, which
  `agentsfleet-runner` never starts.
- `[tools] output_edge_kib` and `max_read_mib`: the executor inside each
  sandbox cuts output and reads to these, in its own process, which cannot
  read this file.
- `[executor] max_frame_mib`: both ends of the executor link frame against it,
  and the end inside the sandbox cannot read this file.
- `[tools] mcp_call_timeout_seconds`: no run reaches an MCP server; the client
  that carries the 300 seconds is not linked into a run's tools.
- `[egress] max_response_mib`: the 1 MiB is also the redaction pass's budget
  and the figure `guffbolt` words its truncation notice by, so the network's
  cap cannot move alone.
- `[egress] max_construction_mib`: the 10 MiB bounds the automaton that masks
  credentials in a redirected response, not a request a tool builds.
- `[egress] max_target_kib`: the 8 KiB bounds a redirect's `Location`, not the
  URL a tool calls.
- `[daemon] max_report_mib` and `max_activity_kib`: no runner code caps a
  report or a live-activity update at these; the figures are a test's payload
  sizes. A report is bounded by the runner link's frame.

## The runner link's frames

The runner reads `max_message_bytes` and `max_control_message_bytes` from the
daemon's welcome and frames every stream of that connection to them: it
refuses to send or read a frame past them. A welcome without them, from a
daemon that predates them, means 32 MiB and 2 MiB, and an announced value
above those is held to them.
