# Logging

This standard covers log records from runtime Rust, through `tracing`, and from TypeScript application code. It sets where records go, their shape, levels, error codes and secrets, and requires a record on every branch an operator or caller can observe. Test diagnostics, build tooling output and framework banners are out of scope.

## Where records go

### LOG-STREAM — Runtime Rust never prints to a stream with `println!`, `eprintln!` or `dbg!`, and logs through `tracing` with an `event` field instead.
- **Applies to:** `**/src/**/*.rs` outside `#[cfg(test)]` items
- **Checked by:** linter: clippy print_stdout, print_stderr, dbg_macro
- **Bad:** `eprintln!("claim failed: {err}")`
- **Good:** `tracing::warn!(event = "claim_failed", error = %err)`
- **Why:** A stream write skips the level, the event and the fields that collectors read.

A program whose interface is stdout, such as a command's answer, uses `#[expect(clippy::print_stdout, reason = "…")]` with a real reason.

### LOG-CONSOLE — TypeScript application code never calls `console.*`, and writes through the repository's logger or output module instead.
- **Applies to:** `*.ts`, `*.tsx`, `*.js` and `*.jsx` application source outside tests, `vendor/` and `node_modules/`
- **Checked by:** linter: oxlint no-console
- **Bad:** `console.error("deploy failed", error)`
- **Good:** `logger.warn("deploy_failed", { error_code: code })`
- **Why:** Console calls skip levels and fields, and they corrupt a command's `--json` output.

Where output is the program's interface, as in a command-line tool's output module, the linter configuration exempts those files.

### LOG-PRETTY — A process picks colored or plain rendering once at startup, and the production stream carries no color codes.
- **Applies to:** logger and `tracing` subscriber setup
- **Checked by:** review
- **Bad:** `A formatter that checks the terminal on every record, or strips color after rendering.`
- **Good:** `fmt::layer().with_ansi(stderr_is_terminal)`, built once when the subscriber is installed
- **Why:** Collectors parse plain records, and rendering a record twice costs time on every line.

## Record shape

### LOG-EVENT — Every production log record carries an `event` field that names what happened in stable snake_case.
- **Applies to:** `tracing` macros in runtime Rust and logger calls in TypeScript application code
- **Checked by:** review
- **Bad:** `tracing::warn!("runner lease failed")`
- **Good:** `tracing::warn!(event = "runner_lease_failed", runner_id = id)`
- **Why:** Dashboards and alerts match on the event, so it cannot live inside free text.

Field shorthand over a local named `event` counts.

### LOG-FIELDS — A record's values are named snake_case fields, never formatted into its message text.
- **Applies to:** `tracing` macros in runtime Rust and logger calls in TypeScript application code
- **Checked by:** review
- **Bad:** `tracing::warn!("lease {id} failed after {ms} ms")`
- **Good:** `tracing::warn!(event = "runner_lease_failed", runner_id = id, duration_ms = ms)`
- **Why:** Collectors filter and count on fields, and message text hides the values.

### LOG-HOIST — A Rust log field computed by a call or expression is bound to a local before the `tracing` macro.
- **Applies to:** `tracing` macros in runtime Rust
- **Checked by:** review
- **Bad:** `tracing::warn!(error_code = error_code::DB_QUERY.as_str(), event = "query_failed")`
- **Good:** `let code = error_code::DB_QUERY.as_str();` before `tracing::warn!(error_code = code, event = "query_failed")`
- **Why:** The `log` bridge expands each field expression twice, and coverage scores the copy that never runs.

Literals and plain locals need no binding.

### LOG-MSG — A record's message is optional detail of at most 300 characters, and a record carries at most 15 fields.
- **Applies to:** every log emit
- **Checked by:** review
- **Bad:** `tracing::error!(event = "deploy_failed", "{backtrace}")`
- **Good:** `tracing::error!(event = "deploy_failed", reason)`, with the trace in its own `debug` record
- **Why:** Long messages bury the event, and a stack trace is diagnosis that belongs at `debug`.

A record leaves the message out when its fields already explain the event.

### LOG-ONELINE — A record is one line, so no field value carries a raw newline.
- **Applies to:** every log emit
- **Checked by:** review
- **Bad:** `summary = %rendered_plan`, where the plan spans many lines
- **Good:** `summary = ?rendered_plan`, whose Debug form escapes each newline as `\n`
- **Why:** A raw newline splits one record into several and breaks every line-based reader.

### LOG-ABSENT — An absent value leaves its field out, so no record carries `key=null` or an empty `key=`.
- **Applies to:** every log emit
- **Checked by:** review
- **Bad:** `region = ?region`, which records `None`
- **Good:** `let region = region.as_deref();` then the `region` field, which `tracing` skips when it is `None`
- **Why:** Parsers disagree on null and empty values, reading them as missing or as text.

### LOG-COLLECTION — A collection is logged as a count with a short sample, or as one record per item.
- **Applies to:** every log emit
- **Checked by:** review
- **Bad:** `tracing::info!(event = "workers_reaped", workers = ?reaped)`
- **Good:** `let count = reaped.len();` then `tracing::info!(event = "workers_reaped", count)`, with one `debug` record per worker
- **Why:** A whole list makes one unbounded record that nobody can scan.

### LOG-RENAME — A port keeps every existing event name byte for byte, and a rename ships as its own intentional change.
- **Applies to:** rewrites of code that already emits events, across languages or crates
- **Checked by:** review
- **Bad:** `runner_lease_failed` becoming `lease_failed` in the rewrite
- **Good:** the rewrite emits `runner_lease_failed`, and any rename is a separate change that updates dashboards
- **Why:** Dashboards and alerts match event names exactly.

## Levels

### LOG-LEVELS — Records use the five levels `error`, `warn`, `info`, `debug` and `trace`, and no others.
- **Applies to:** every log emit and logger configuration
- **Checked by:** review
- **Bad:** `logger.log("fatal", …)`, or a `notice` level
- **Good:** `tracing::error!` for the failure that ends the process
- **Why:** New level names fall outside every collector's filters and dashboards.

### LOG-PAIR — An operation that crosses a process, network, datastore or subprocess boundary logs `<op>_started` and exactly one `<op>_completed` or `<op>_failed` on every exit path.
- **Applies to:** boot steps, migrations, lease handling, outbound vendor calls, subprocess runs, and subsystem start and stop
- **Checked by:** review
- **Bad:** `migration_started` logged, then a `?` returns early with no `migration_failed`
- **Good:** the error arm logs `migration_failed` with the identifier the start record carried
- **Why:** Without both halves, a hang looks like work never reached, and duration and repeats go unmeasured.

Pure computations and accessors get no pair, since the rule binds work that can fail slowly.
Adding the missing half of a pair is always in scope.

### LOG-INFO — Any state change an engineer judges significant may log at `info`, with the identifiers, counts, outcomes and durations that make it actionable.
- **Applies to:** `info` records
- **Checked by:** review
- **Bad:** `tracing::info!(event = "config_loaded")` with nothing else
- **Good:** `tracing::info!(event = "config_loaded", source = "env:DATABASE_URL", node_count)`
- **Why:** A missing `info` record can cost an incident nobody can trace, while an extra one costs a line.

No allow-list of event names exists, and a request to remove an `info` record names a concrete cost.

### LOG-HOT — A per-iteration path, such as a loop body, a per-row step or a per-chunk callback, logs at `debug`, and its aggregate outcome logs at `info`.
- **Applies to:** loops, sweeps, polls and stream callbacks
- **Checked by:** review
- **Bad:** `tracing::info!` for each of 4,000 reaped workspaces
- **Good:** a `debug` record per workspace and one `info` record with the count
- **Why:** Per-item `info` records flood collectors and drown the records that matter.

A hot poll that crosses the key-value store and Postgres on every pass keeps its LOG-PAIR records, both at `debug`.

### LOG-FAIL — A path that can fail logs its failure at `warn` or `error`, in every build and environment.
- **Applies to:** error branches, `catch` blocks and ignored results in runtime code
- **Checked by:** review
- **Bad:** `let _ = fs::remove_file(&lock_path);`
- **Good:** `if let Err(error) = fs::remove_file(&lock_path) { tracing::warn!(event = "lock_remove_failed", %error) }`
- **Why:** A silent failure leaves operators nothing to search for.

A condition that ends the process logs `error` just before exit, so the last line names the reason.

## Error codes

### LOG-CODE — Every `warn` or `error` record for a domain failure carries `error_code` from the repository's error registry.
- **Applies to:** `warn` and `error` records in runtime code
- **Checked by:** review
- **Bad:** `tracing::error!(event = "invite_rejected", reason)`
- **Good:** `let code = error_code::INVITE_EXPIRED.as_str();` before `tracing::error!(error_code = code, event = "invite_rejected")`
- **Why:** The code links a log line to its documented cause and fix.

A new code's registry entry lands in the same commit as its first use.
A system failure with no domain meaning carries no code until a user sees it, and the boundary that shows it attaches one.

### LOG-CAUSE — A record about a decision on another failure names its own code in `error_code` and the other failure's code in a `<role>_error_code` field.
- **Applies to:** records for fail-open gates, rollbacks and refused configuration
- **Checked by:** review
- **Bad:** `error_code = cause_code` on the record that reports the fallback
- **Good:** `error_code = fallback_code, cause_error_code = cause_code`
- **Why:** Overwriting the record's own code hides which decision the record reports.

Roles in use are `cause_error_code`, `rollback_error_code` and `config_error_code`, and a new role keeps the same shape.
A code is a field, never part of a reason sentence.

## Secrets

### LOG-SECRET — No log record at any level carries a secret value, and a record that needs a secret's origin logs its source.
- **Applies to:** Postgres connection strings, key-value store URLs, OAuth tokens, provider keys and tenant-supplied secrets
- **Checked by:** review
- **Bad:** `tracing::debug!(event = "provider_configured", api_key = %key)`
- **Good:** `tracing::debug!(event = "provider_configured", source = "env:OPENAI_API_KEY")`
- **Why:** `debug` runs exactly when something is already wrong, and a leaked secret cannot be recalled.

Redaction covers every stream the changed operation writes, stdout and stderr alike.
When a value might be secret, the field is left out.

### LOG-REDACT — An error that carries untrusted or secret input passes through a filter that keeps names and drops values before it reaches a log, a reply or a model.
- **Applies to:** `serde` type errors, paths that inline key material, child process argv, and library traces of request or response bodies
- **Checked by:** review
- **Bad:** `tracing::warn!(event = "body_rejected", error = %serde_err)`
- **Good:** `let field = unknown_field_of(&serde_err);` before `tracing::warn!(event = "body_rejected", field)`
- **Why:** A type error quotes the value it rejected, and request bodies carry keys and tokens.

## Observable branches

### OBS — Every branch that changes externally observable behavior emits a structured event for operators, callers or downstream consumers.
- **Applies to:** handlers, middleware, workers, commands, lifecycle code, retries and fallbacks in runtime source
- **Checked by:** review
- **Bad:** `if seen { return Ok(Reply::Deduped); }`
- **Good:** `if seen { tracing::info!(event = "webhook_deduped", delivery); return Ok(Reply::Deduped); }`
- **Why:** A silent branch is invisible in dashboards, so correct code can still be opaque to operators.

Branches that fire it include a non-default result, an error turned into a response or exit code, and success with a diagnostic body.
So do a swallowed error, an early return before the main side effect, a configuration switch, and each retry with its attempt number.
A request-path record carries the request identifier, and a command writes each non-success code to stderr or its `--json` output.
