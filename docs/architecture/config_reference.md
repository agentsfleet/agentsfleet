# `agentsfleetd` and `agentsfleet-runner` configuration reference

`agentsfleetd` reads one TOML file, named by `--config FILE` or `AGENTSFLEET_CONFIG_FILE`.
`agentsfleet-runner` reads one TOML file, named by `AGENTSFLEET_RUNNER_CONFIG_FILE`.
Every key is optional, and each default below is the value in code today.

The daemon's parser is `rustd/crates/daemon/main/src/config/` (`settings.rs`, `tuning.rs`, `mod.rs`).
The runner's parser is `rustd/crates/runner/main/src/config/` (`mod.rs`, `tuning.rs`).
The Fly files in `deploy/fly/` and `deploy/baremetal/agentsfleet-runner.toml` carry the tuning knobs below with comments.

## Tuning for performance

Start here when a deployment is slow or refusing work under load.
Each row reads as: what you see, the key, which way to move it, and what the move costs.

| You see | Key | Move | Cost |
|---|---|---|---|
| Callers get `429` with `UZ-API-001` and `Retry-After: 1`, while CPU and the pool have room | `[server] max_in_flight_requests` (256) | Raise | More requests compete for CPU and database connections at once |
| Requests get slower as load rises, but few are refused | `[server] max_in_flight_requests` | Lower | More callers get `429` and retry |
| Dashboards get `503` with `UZ-API-002` when opening live streams | `[server] max_sse_streams` (256) | Raise | Each open stream holds memory for minutes |
| Requests wait for a database connection; Postgres has free slots | `[database] max_connections` (20), or `[database.api] max_connections` | Raise | Uses Postgres slots; keep the sum across replicas under Postgres `max_connections` |
| The first requests after a quiet spell are slow | `[database] min_connections` (unset: a quarter of the pool, 5 of 20) | Raise | Idle connections sit open on Postgres |
| Short load spikes fail with `UZ-INTERNAL-001` | `[database] acquire_timeout_seconds` (5) | Raise | Callers wait longer before they see a failure |
| Turns wait while runners sit idle | `[runners] turns_per_assignment_pass` (64) | Raise | Each assignment pass reads more rows |
| After a NATS wake link restart or a garbled wake-up, waiting work reaches runners slowly | `[runner.scheduling] max_concurrent_recovery_assignments` (8) | Raise | That catch-up puts more load on the database at once |
| `agentsfleet_fleet_ready_wake_queue_events_total` shows `evicted` wake-ups | `[nats] wake_queue_capacity_fleets` (unset: `default_wake_queue_size`, 64) | Raise | More memory per daemon; evicted work is not lost, it stays in Postgres |
| New messages are refused during a burst that runners can absorb | `[turns] max_queued_turns` (100,000) | Raise | A larger backlog waits longer for a runner |
| Clients page through long lists one small page at a time | `[api] default_page_size` (50) | Raise | Each list read returns more rows |
| One large list read slows the API or the database | `[api] max_page_size` (100) | Lower | Clients need more requests for a full list |
| Runner metrics show the runner label `_other` | `[telemetry] max_metric_series` (4,096) | Raise | The metrics backend stores more series |
| Runs die out of memory | Runner `[sandbox] memory_mib` (2,048) | Raise | Fewer runs fit on one host |
| Runs fail with a full disk | Runner `[sandbox] disk_mib` (4,096) | Raise | Fewer runs fit on the host's disk |
| Builds and installs inside runs are slow on disk | Runner `[sandbox] io_mib_per_second` (200) | Raise | One busy run can slow the other runs on the host |
| Long model replies fail mid-stream after a silence | Runner `[inference] stream_idle_timeout_seconds` (300) | Raise | A stuck provider holds the run longer |
| Decision calls time out on a slow model | Runner `[provider] decision_call_timeout_seconds` (12) | Raise | A stuck call holds the run longer |

Two knobs people look for have no file key today.
A runner's worker count comes from the daemon over the runner link (`afr_supervisor/src/worker_pool/mod.rs`).
Command output is cut to 512 kibibytes (KiB) from each end by a constant shared by `guffbolt` and the in-sandbox process (`EDGE_BYTES`).

## `agentsfleetd` keys

Sizes use mebibytes (MiB) and kibibytes (KiB). A count, size or timeout refuses `0` unless the table says otherwise.

### `[server]`

| Key | Default | Change it when… |
|---|---|---|
| `port` | `PORT` | You serve on another port; `serve --port` wins over it |
| `api_url` | `API_URL` | The public API address changes |
| `app_url` | `APP_URL` | The public app address changes |
| `max_in_flight_requests` | 256 | Callers get `429 UZ-API-001` while the machine has room; lower it when latency climbs |
| `max_sse_streams` | 256 | Dashboards get `503 UZ-API-002`; lower it when streams use too much memory |
| `drain_timeout_seconds` | 15 | Shutdown cuts off requests that were close to done |
| `background_task_join_timeout_seconds` | 10 | Shutdown logs background tasks that did not stop in time |
| `max_live_activity_notices` | 16 | Live activity on dashboards lags behind runs |
| `max_request_body_mib` | 1 | Senders get `413` for bodies they need to send, such as webhook deliveries; lower it when large bodies use too much memory. A route that carries chat text or a runner frame reads the larger size that text or frame needs |
| `read_timeout_seconds` | 300 | Clients on slow links are dropped while still sending a request; lower it when stalled connections hold sockets too long. It is the time a client gets to send a request's headers, and then its body; past it the connection is dropped. It never cuts a response, so live streams are unaffected. An idle HTTP/1 keep-alive connection closes after the same time |

### `[api]`

| Key | Default | Change it when… |
|---|---|---|
| `default_page_size` | 50 | Clients that name no size need more or fewer rows |
| `max_page_size` | 100 | Large list reads slow the database, or clients need bigger pages |
| `default_thread_page_size` | 25 | The chat view loads too few or too many messages at first |
| `max_thread_page_size` | 100 | Large chat reads slow the database |

### `[database]`

These are defaults for both roles. The environment and the role sections can win over them; see Boot checks.

| Key | Default | Change it when… |
|---|---|---|
| `max_connections` | 20 | Requests wait for connections and Postgres has free slots |
| `min_connections` | unset: a quarter of `max_connections` | The first requests after idle are slow |
| `acquire_timeout_seconds` | 5 | Load spikes fail with `UZ-INTERNAL-001` |
| `connect_timeout_seconds` | 10 | Startup fails to reach a slow or distant Postgres |
| `idle_timeout_minutes` | 10 | Idle connections crowd Postgres, or reconnects are frequent |
| `max_connection_lifetime_minutes` | 30 | A Postgres failover takes too long to be picked up |

### `[database.api]` and `[database.migrator]`

Each key overrides `[database]` and the environment for that role only.

| Key | Default | Change it when… |
|---|---|---|
| `max_connections` | from `[database]` | One role needs its own pool size |
| `min_connections` | from `[database]` | One role needs its own warm floor; the migrator ships with 0 |
| `acquire_timeout_ms` | from `[database]` | One role should wait longer or shorter for a connection |
| `connect_timeout_ms` | from `[database]` | One role reaches Postgres over a slower path |
| `idle_timeout_seconds` | from `[database]` | One role holds idle connections too long |
| `max_lifetime_seconds` | from `[database]` | One role should pick up a failover sooner |
| `ping_after_idle_ms` | 1,000 | Connections that sat idle fail on first use |

### `[database.migration]`

| Key | Default | Change it when… |
|---|---|---|
| `lock_max_attempts` | 30 | Deploys fail because another replica holds the migration lock |
| `lock_retry_interval_ms` | 1,000 | You want lock polls closer together or further apart |

### `[runner.listener]`

| Key | Default | Change it when… |
|---|---|---|
| `address` | `[::]:4443` | Runners must reach the daemon on another address or port |
| `tls_cert_file` | required | The certificate chain moves |
| `tls_key_file` | required | The private key moves |
| `max_udp_payload_bytes` | 1,232 | The network path carries larger packets than a 1,280-byte IPv6 path |

### `[runner.stream.control]` and `[runner.stream.data]`

| Key | Default | Change it when… |
|---|---|---|
| `control.max_buffered_bytes` | 16 MiB | Heartbeats and renewals queue behind each other under many runners |
| `data.max_buffered_bytes` | 64 MiB | Work offers and reports queue under many busy runners |

### `[runner.run]`, `[runner.health]`, `[runner.sandboxes]`

| Key | Default | Change it when… |
|---|---|---|
| `run.ttl_seconds` | 35 | A run's lease lapses on a slow link; shorter finds a dead runner sooner |
| `health.heartbeat_interval_seconds` | 10 | Runners should report more or less often |
| `health.offline_after_seconds` | unset: 3 × `ttl_seconds` (105) | Runners are marked offline too fast or too slow |
| `health.healthy_before_uncordon_seconds` | 30 | A recovered runner should get work sooner or later |
| `sandboxes.max_idle_hold_seconds` | 600 | Frozen sandboxes hold host resources too long, or follow-ups miss a warm sandbox |

### `[runner.recovery]` and `[runner.scheduling]`

| Key | Default | Change it when… |
|---|---|---|
| `recovery.resume_from_checkpoint` | `false` | You want a lost run to resume from its checkpoint |
| `scheduling.max_concurrent_recovery_assignments` | 8 | Catch-up after a wake link restart is slow, or loads the database |

### `[runners]`

| Key | Default | Change it when… |
|---|---|---|
| `max_message_mib` | 32 | You want runners to frame smaller messages; it cannot go above 32 |
| `max_control_message_mib` | 2 | You want runners to frame smaller control messages; it cannot go above 2 |
| `turns_per_assignment_pass` | 64 | Turns wait while runners sit idle, or each pass reads slowly |
| `max_subscription_repair_backoff_seconds` | 5 | Bus subscriptions take too long to come back after a NATS hiccup |

The daemon sends `max_message_mib` and `max_control_message_mib` to each runner when it connects.

### `[nats]`

| Key | Default | Change it when… |
|---|---|---|
| `dev_namespace` | `AGENTSFLEET_DEV_NAMESPACE` | Several developers share one NATS cluster |
| `wake_queue_capacity_fleets` | `AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY`, then `default_wake_queue_size` | Wake-ups are evicted under load |
| `default_wake_queue_size` | 64 | You want another queue size when neither of the above names one |
| `max_wake_queue_size` | 65,536 | You need a wake queue larger than 65,536 |

### `[turns]` and `[turns.checkpoints]`

| Key | Default | Change it when… |
|---|---|---|
| `max_runs_per_turn` | 3 | Turns fail after runner crashes that a further try would survive |
| `max_queued_turns` | 100,000 | New messages are refused in a burst, or a backlog waits too long |
| `history_turns_per_page` | 100 | Loading chat history for a run is slow |
| `checkpoints.retention_days` | 7 | No effect today; see below |
| `checkpoints.transfer_window_minutes` | 15 | Large checkpoint uploads or restores run out of time |
| `checkpoints.restore_pin_minutes` | 2 | A restore loses its checkpoint to deletion mid-way |
| `checkpoints.orphan_after_minutes` | 60 | Uploads that never finished hold storage too long |

### `[context]`

| Key | Default | Change it when… |
|---|---|---|
| `small_model_max_tokens` | 200,000 | Models are put in the wrong tool-window class |
| `large_model_min_tokens` | 1,000,000 | Models are put in the wrong tool-window class |
| `small_tool_window_count` | 10 | Small models run out of context, or forget recent tool calls |
| `default_tool_window_count` | 20 | Mid-size models run out of context, or forget recent tool calls |
| `large_tool_window_count` | 30 | Large models could keep more tool history |

### `[fleets]` and `[billing]`

| Key | Default | Change it when… |
|---|---|---|
| `fleets.definition_cache_mib` | 64 | Many fleets re-parse their definitions on each run |
| `fleets.head_cache_count` | 16,384 | You run more fleets than the cache holds |
| `billing.price_book_cache_count` | 1,024 | You meter more models than the cache holds |

### `[durable_tasks]` and `[durable_tasks.checkpoints]`

| Key | Default | Change it when… |
|---|---|---|
| `slice_seconds` | 600 | Tasks should check their funds more or less often |
| `operation_timeout_seconds` | 30 | GitHub reads or storage calls inside a task time out |
| `terminal_retention_days` | 7 | No effect today; see below |
| `checkpoints.keep_generations` | 3 | No effect today; see below |
| `checkpoints.upload_link_ttl_minutes` | 5 | Runners' upload links expire before large uploads start |
| `checkpoints.download_link_ttl_minutes` | 5 | Runners' download links expire before large downloads start |
| `checkpoints.transfer_window_minutes` | 15 | Large task checkpoints run out of time |
| `checkpoints.restore_pin_minutes` | 30 | A restore loses its checkpoint to deletion mid-way |
| `checkpoints.orphan_after_minutes` | 60 | Uploads that never finished hold storage too long |
| `checkpoints.objects_per_delete_call` | 256 | No effect today; see below |

Four checkpoint keys are read only by checkpoint maintenance, and no daemon code starts that maintenance today.
They are `[turns.checkpoints] retention_days`, `[durable_tasks] terminal_retention_days`, `keep_generations` and `objects_per_delete_call`.
Only the integration tests call `maintain_checkpoints`, so setting these keys changes nothing in a running daemon.

### `[fleet.durable_tasks]` and `[fleet.durable_tasks.defaults]`

Unset, each `[fleet.durable_tasks]` limit allows any amount. The defaults apply to new objectives only.

| Key | Default | Change it when… |
|---|---|---|
| `max_unfinished_per_fleet` | no limit | One fleet opens too many objectives at once |
| `max_inputs_per_safe_boundary` | no limit | One read recovers too many inputs |
| `max_actions_per_message` | no limit | One message carries too many actions |
| `max_objective_bytes` | no limit | Objectives are too large to store |
| `max_title_bytes` | no limit | Titles are too long for the interface |
| `max_completion_questions` | no limit | Objectives carry too many completion questions |
| `max_question_text_bytes` | no limit | Completion questions are too long |
| `max_authored_question_bytes` | no limit | A question and its criteria together are too long |
| `defaults.model_budget_nanodollars` | 5,000,000,000 | New objectives need a larger or smaller model allowance |
| `defaults.wall_seconds` | 604,800 | New objectives should live longer or shorter |
| `defaults.running_seconds` | 7,200 | New objectives need more or less active time |
| `defaults.max_runs` | 12 | New objectives need more or fewer runs |
| `defaults.max_publication_attempts` | 8 | New objectives need more or fewer publication tries |
| `defaults.max_decision_calls` | 128 | New objectives need more or fewer decision calls |

### `[setup]`

| Key | Default | Change it when… |
|---|---|---|
| `request_timeout_seconds` | 15 | Setup requests are answered as busy on a slow model |
| `replay_window_hours` | 24 | A repeated setup request should replay its first answer for longer or shorter |
| `maintenance_drafts_per_page` | 64 | Setup drafts wait while runners sit idle |

Setup maintenance no longer exists, so the name of `maintenance_drafts_per_page` is stale.
The key now caps the setup drafts offered to a runner each time it asks for work (`afd_setup/src/setup_run/mod.rs`).

### `[auth]`

| Key | Default | Change it when… |
|---|---|---|
| `cli_login_ttl_seconds` | 300 | People need more time to approve a CLI login |
| `cli_login_code_replay_seconds` | 60 | A login code should be refused for reuse for longer |
| `max_cli_login_code_attempts` | 5 | People mistype codes often; it cannot go above 10 |
| `oauth_state_ttl_minutes` | 10 | Connector sign-in links expire before people finish |
| `identity_timeout_seconds` | 5 | Calls to the identity provider time out |
| `jwks_fetch_timeout_seconds` | 5 | Fetching signing keys times out |
| `jwks_cache_hours` | 6 | The identity provider rotates keys more often |
| `jwks_stale_grace_minutes` | 15 | Sign-in fails during a short identity provider outage |
| `jwks_min_refresh_seconds` | 30 | Unknown keys cause too many refetches |

### `[credentials]`

| Key | Default | Change it when… |
|---|---|---|
| `token_cache_count` | 8,192 | You hold more live tokens than the cache keeps |
| `platform_key_cache_minutes` | 5 | A rotated platform model key should reach runs sooner |
| `assumed_token_ttl_minutes` | 5 | A provider's tokens without an expiry live longer or shorter |

### `[connectors]`

| Key | Default | Change it when… |
|---|---|---|
| `slack_api_url` | `SLACK_API_URL` | Tests point Slack calls at a stub |
| `github.installation_token_ttl_minutes` | 60 | You want GitHub tokens renewed sooner; it cannot go above 60 |
| `slack.post_timeout_seconds` | 5 | Posting answers to Slack times out |
| `slack.answer_check_timeout_seconds` | 3 | Checking for an earlier answer times out |
| `slack.thread_read_timeout_ms` | 1,500 | Reading Slack threads times out |
| `mail.send_timeout_seconds` | 10 | Invite emails time out |
| `qstash.attempt_timeout_ms` | 2,500 | Calls to QStash, the schedule service, time out |
| `qstash.call_budget_seconds` | 10 | All QStash tries together need more or less time |
| `qstash.max_attempts` | 3 | QStash calls fail on short outages |
| `qstash.max_retry_delay_ms` | 800 | QStash tries should wait longer between each other |

### `[outbound.retries]`

| Key | Default | Change it when… |
|---|---|---|
| `max_attempts` | 3 | Answers to Slack and other vendors fail on short outages |
| `initial_backoff_ms` | 200 | The first retry comes too soon or too late |
| `max_backoff_ms` | 800 | Retries wait too long; each wait delays the answers behind it |

### `[identity]`

| Key | Default | Change it when… |
|---|---|---|
| `issuer` | `OIDC_ISSUER` | The identity provider changes |
| `audience` | `OIDC_AUDIENCE` | The API's public address changes |
| `jwks_url` | `OIDC_JWKS_URL` | Signing keys move |
| `clerk_api_base` | `CLERK_API_BASE` | The Clerk API address changes |
| `platform_admin_workspace_id` | `PLATFORM_ADMIN_WORKSPACE_ID` | The platform admin workspace changes |

### `[telemetry]` and `[telemetry.otlp]`

OpenTelemetry Protocol (OTLP) is the export format for traces, logs and metrics.

| Key | Default | Change it when… |
|---|---|---|
| `log_level` | `AGENTSFLEET_LOG_LEVEL` | You need more or less log detail |
| `posthog_host` | `POSTHOG_HOST` | Product analytics go to another host |
| `otlp_endpoint` | `OTEL_EXPORTER_OTLP_ENDPOINT` | The collector moves |
| `otlp_protocol` | `OTEL_EXPORTER_OTLP_PROTOCOL` | The collector speaks another protocol |
| `max_metric_series` | 4,096 | Runner metrics show runner `_other` |
| `metric_max_age_seconds` | 120 | Model catalogue metrics drop out because their snapshot is older than this |
| `otlp.metrics_export_interval_seconds` | 60 | Dashboards need fresher metrics, or export costs too much |
| `otlp.trace_export_interval_seconds` | 5 | Traces arrive late |
| `otlp.log_export_interval_seconds` | 1 | Logs arrive late, or export too often |
| `otlp.export_timeout_seconds` | SDK default | Exports to a slow collector time out |
| `otlp.trace_queue_capacity` | 2,048 | Spans are dropped under load |
| `otlp.trace_batch_size` | 512 | The collector refuses large batches |
| `otlp.log_queue_capacity` | 2,048 | Log records are dropped under load |
| `otlp.log_batch_size` | 512 | The collector refuses large batches |

### `[checkpoint.storage]`

Unset, the daemon stores no task checkpoints in R2, Cloudflare's object storage.

| Key | Default | Change it when… |
|---|---|---|
| `endpoint` | none | You turn on checkpoint storage; an `r2.cloudflarestorage.com` account address |
| `bucket` | none | You turn on checkpoint storage |
| `credential_name` | none | The vault row holding the R2 credentials changes |

## `agentsfleet-runner` keys

### `[inference]`, `[provider]`, `[provider_retries]`

| Key | Default | Change it when… |
|---|---|---|
| `inference.connect_timeout_seconds` | 10 | Model calls fail to connect over a slow network |
| `inference.stream_idle_timeout_seconds` | 300 | Long replies fail after a silence mid-stream |
| `provider.decision_connect_timeout_seconds` | 3 | Decision calls fail to connect |
| `provider.decision_call_timeout_seconds` | 12 | Decision calls time out on a slow model |
| `provider.token_count_timeout_seconds` | 10 | Token-count calls time out |
| `provider.max_token_count_reply_kib` | 16 | Token-count replies are refused as too large |
| `provider_retries.max_attempts` | 3 | Model calls fail on short provider errors |
| `provider_retries.max_stream_open_attempts` | 3 | Streams fail to open on short provider errors |
| `provider_retries.initial_backoff_ms` | 1,000 | The first retry comes too soon or too late |
| `provider_retries.max_retry_after_seconds` | 60 | Providers ask for longer waits that you want to honor |

### `[supervisor]` and `[daemon]`

| Key | Default | Change it when… |
|---|---|---|
| `supervisor.daemon_max_attempts` | 5 | Calls to the daemon fail on short outages |
| `supervisor.daemon_initial_backoff_ms` | 250 | Reconnects start too soon or too late |
| `supervisor.daemon_max_backoff_ms` | 30,000 | Reconnects wait too long after an outage |
| `supervisor.max_concurrent_file_operations` | 2 | Checkpoint captures are skipped because both slots are busy |
| `daemon.call_timeout_seconds` | 20 | Calls to a busy daemon time out |
| `daemon.max_activity_batches_held` | 4 | Live activity is dropped while the daemon is busy |
| `daemon.max_activity_batch_kib` | 64 | Live activity updates are split too finely |

### `[sandbox]`

| Key | Default | Change it when… |
|---|---|---|
| `memory_mib` | 2,048 | Runs die out of memory, or you want more runs per host |
| `disk_mib` | 4,096 | Runs fill their disk, or you want more runs per host |
| `io_mib_per_second` | 200 | Disk-heavy runs are slow, or one run starves the others |
| `ready_timeout_seconds` | 10 | New sandboxes fail to start in time on a slow host |

### `[tools]`

| Key | Default | Change it when… |
|---|---|---|
| `default_shell_timeout_seconds` | 10 | Commands without a timeout are cut off too early |
| `min_command_output_wait_ms` | 250 | Models poll running commands too often |
| `max_command_output_wait_ms` | 30,000 | Models wait too long for output from running commands |
| `max_shell_processes_per_run` | 64 | Runs need more interactive shells, or use too many |
| `default_file_read_lines` | 2,000 | File reads without a limit return too much or too little |
| `max_image_dimension_px` | 2,048 | Images reach the model too large or too blurry |
| `max_image_mib` | 5 | Images are refused as too large |
| `max_sleep_hours` | 12 | Runs need to sleep longer, or sleep too long |
| `max_subagent_wait_seconds` | 3,600 | Runs give up on slow subagents too soon |
| `max_open_subagents` | 6 | Runs need more subagents at once, or use too many |
| `max_subagent_depth` | 2 | You want subagents to start fewer levels of their own |
| `max_file_read_mib` | 8 | File tools refuse or cut files the model needs whole, or one read holds too much memory |
| `output_edge_kib` | 512 | Command output loses the middle the model needs, or each running command holds too much memory |

### `[executor]`

The runner starts each sandbox's executor with these sizes and holds its own end of the link to the same.

| Key | Default | Change it when… |
|---|---|---|
| `max_frame_mib` | 64 | File writes are refused as longer than the executor reads, or a hostile sandbox could make the runner buffer too much |

### `[egress]` and `[telemetry]`

| Key | Default | Change it when… |
|---|---|---|
| `egress.connect_timeout_seconds` | 10 | Web fetches fail to connect to slow sites |
| `egress.request_timeout_seconds` | 30 | Web fetches time out on slow sites |
| `egress.max_redirects` | 3 | Sites need longer redirect chains |
| `egress.max_secret_mask_mib` | 10 | Redirected downloads carrying many signed URLs are refused because the matcher that masks their secrets ran out of room |
| `telemetry.max_spans_per_run` | 256 | Long runs lose trace detail, or export too much |

## Boot checks

`agentsfleetd` refuses a bad file before it opens a socket.
It names the key as `AGENTSFLEET_CONFIG_FILE is set but unusable` with `UZ-STARTUP-001`.
`agentsfleet-runner run` prints the same code and exits before it exports or dials the daemon.

Both binaries refuse:

- an unknown key or section, including the retired `[maintenance]`, `[inference]` and `[authoring.turn]` daemon sections;
- a count, size or timeout of `0`;
- a default above its ceiling: page sizes, wake queue sizes, and `min_connections` above `max_connections`;
- `small_model_max_tokens` at or above `large_model_min_tokens`;
- a URL that is not absolute HTTP or HTTPS, or that embeds a user or password.

The daemon also refuses:

- `max_in_flight_requests` or `max_sse_streams` over 65,536, and `max_concurrent_recovery_assignments` over 64;
- `drain_timeout_seconds` under 6, and `background_task_join_timeout_seconds` under 9;
- `max_message_mib` over 32, and `max_control_message_mib` over 2;
- `ttl_seconds` outside 33 to 3,600, a heartbeat that does not fit twice inside the offline time, or an uncordon time outside them;
- `max_udp_payload_bytes` outside 1,200 to 65,507, a control buffer under 12 MiB, or a data buffer under 64 MiB;
- `lock_max_attempts` over 1,000, or attempts times `lock_retry_interval_ms` over 300,000;
- `[outbound.retries]` with more than 10 attempts or a backoff over 30,000 ms;
- `[connectors.qstash]` tries and waits that cannot end inside `call_budget_seconds`, a budget of 30 seconds or more, or `max_retry_delay_ms` under 200;
- `[auth]` values that weaken sign-in: lifetimes over one day, provider calls over 60 seconds, or more than 10 wrong codes;
- `installation_token_ttl_minutes` over 60, GitHub's own ceiling.

The runner also refuses:

- `min_command_output_wait_ms` above `max_command_output_wait_ms`, and `[egress] connect_timeout_seconds` above `request_timeout_seconds`;
- `default_shell_timeout_seconds` over 2,147,483, the longest timer tokio holds;
- `max_shell_processes_per_run` of 8 or fewer, since the 8 newest shells are never closed to make room;
- `max_subagent_depth` over 2, and `[daemon] call_timeout_seconds` of 12 or less;
- `max_activity_batches_held` under 2, one in flight and one queued;
- `[executor] max_frame_mib` under 7, the longest a full directory listing serializes to, or at most four thirds of `[tools] max_file_read_mib`, since one read travels as base64.

### Precedence

The runner reads only its file. An unset runner key takes the constant in code.

Most daemon keys work the same way: the file sets the value, and the constant is only the default.
Some older daemon keys also have an environment variable, shown in their Default column.
For those keys the file wins, then the variable, then the constant (`config/values.rs`, `config/mod.rs`).

`[database]` is the exception, because deployments still set pool sizes in the environment.
The order is `[database.api]` or `[database.migrator]`, then `DATABASE_POOL_SIZE`, `DATABASE_MIN_POOL_SIZE` and `DATABASE_ACQUIRE_TIMEOUT_MS` with their `_API` and `_MIGRATOR` forms, then `[database]`.
When nothing names `min_connections`, a quarter of the resolved pool stays open.
