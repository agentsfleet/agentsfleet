# M23_001: CLI UI Module — Icons, Colors, Spinners, and Structured Error Display

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 001
**Date:** Apr 02, 2026
**Status:** NOT_DONE
**Priority:** P2 — nice-to-have DX parity, but zombied/executor are agent-consumed, not human-facing
**Batch:** B1
**Branch:**
**Depends on:** None
**Reason:** Adds complexity for minimal return. zombied and zombied-executor are primarily consumed by agents and infrastructure automation, not humans at a terminal. The structured log format (`ts_ms=... level=... scope=...`) is correct for Loki/Grafana observability. Icons, colors, and spinners would add ~200 lines of Zig code touching 7+ files for output that almost no human sees. Revisit only if zombied gains interactive CLI subcommands aimed at developers.

---

## Goal

Add a shared `src/ui.zig` module used by both `zombied` and `zombied-executor` that provides Unicode icons, ANSI colors, a braille spinner, and structured error display. The module is TTY-aware: when stderr is not a TTY or `NO_COLOR` is set, icons render plain (no ANSI escapes) and the spinner is disabled. Structured machine-readable log lines (`ts_ms=... level=... scope=...`) remain untouched — the UI layer writes separate human-facing lines.

---

## 1.0 Core UI Module (`src/ui.zig`)

**Status:** PENDING

A single file, zero dependencies beyond `std`. Provides four capabilities: icons, colors, spinner, and error formatting.

**Dimensions:**
- 1.1 PENDING Icon constants: `ok` (✔), `err` (✖), `info` (ℹ), `warn` (▲), `run` (◉), `dot` (·) — exported as `pub const`
- 1.2 PENDING Color functions: `green()`, `red()`, `cyan()`, `yellow()`, `bold()`, `dim()` — wrap text in ANSI escapes; return plain text when `NO_COLOR` is set or stderr is not a TTY
- 1.3 PENDING TTY/NO_COLOR detection: check `std.posix.isatty(std.io.getStdErr().handle)` and `std.posix.getenv("NO_COLOR")` once at init; gate all color and spinner output on the result
- 1.4 PENDING `styled()` combiner: `ui.styled(.ok, "migrate complete")` → `"\x1b[32m✔ migrate complete\x1b[0m"` (or plain `"✔ migrate complete"` when no color)

---

## 2.0 Spinner (`src/ui.zig` — Spinner struct)

**Status:** PENDING

A braille-frame spinner that overwrites a single stderr line using `\r`. Runs on a background `std.Thread`. API: `start(label)` → returns handle; `stop(.ok | .err | .warn)` joins thread and prints final icon + label.

**Dimensions:**
- 2.1 PENDING Spinner frames: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` cycling at ~80ms interval
- 2.2 PENDING `Spinner.start(stderr_writer, label)` spawns a thread that writes `\r{frame} {label}` to stderr; returns `Spinner` handle
- 2.3 PENDING `Spinner.stop(outcome)` sets atomic bool, joins thread, clears line with `\r`, prints final line: `{icon} {label}\n` using the outcome's icon (ok→✔, err→✖, warn→▲)
- 2.4 PENDING When TTY detection is false, `start()` returns a no-op Spinner that prints `ℹ {label}...` once on start and `{icon} {label}` on stop — no animation, no thread

---

## 3.0 Structured Error Display

**Status:** PENDING

A helper that renders multi-line error blocks with icon, error name, hint, and docs URL. Integrates with the existing `error_codes` module.

**Dimensions:**
- 3.1 PENDING `ui.printError(writer, code, err, extra_context)` renders: `✖ {context} ({code})\n  err: {error_name}\n  hint: {hint}\n  docs: {docs_url}`
- 3.2 PENDING Hint and docs URL sourced from `src/errors/codes.zig` via existing `error_codes.hint()` and `error_codes.ERROR_DOCS_BASE`
- 3.3 PENDING When no hint exists for the code, omit the hint line (don't print empty)

---

## 4.0 Integration — zombied Commands

**Status:** PENDING

Wire `ui.zig` into each zombied subcommand's user-facing output paths. Structured log lines remain unchanged.

### 4.1 `migrate.zig`

**Dimensions:**
- 4.1.1 PENDING Spinner during DB connect wait: start before `initFromEnvForRole`, stop on success or final failure
- 4.1.2 PENDING Retry warn lines use `ui.styled(.warn, ...)` instead of raw `log.warn`
- 4.1.3 PENDING Final success: `ui.styled(.ok, "migrations complete")` to stderr

### 4.2 `doctor.zig`

**Dimensions:**
- 4.2.1 PENDING Replace `[OK]`/`[FAIL]` brackets with `✔`/`✖` icons with color
- 4.2.2 PENDING Summary line: green `✔ All checks passed.` or red `✖ N/M checks passed`

### 4.3 `run.zig`

**Dimensions:**
- 4.3.1 PENDING Spinner during SSE connect in `--watch` mode
- 4.3.2 PENDING Gate results: `✔ gate_name` (pass) / `✖ gate_name` (fail) with color
- 4.3.3 PENDING `[done]` replaced with `✔ run complete`

### 4.4 `serve.zig` startup

**Dimensions:**
- 4.4.1 PENDING Each startup phase (env_check, config_load, redis_connect, sandbox_preflight) gets `✔` on success, `✖` on failure
- 4.4.2 PENDING Final "http.server_starting" line uses `ui.styled(.ok, ...)`

### 4.5 `worker.zig` startup/drain

**Dimensions:**
- 4.5.1 PENDING Startup phases get icons matching serve pattern
- 4.5.2 PENDING Drain messages: `▲ draining...`, `✔ drain complete`, `▲ drain timeout`

---

## 5.0 Integration — zombied-executor

**Status:** PENDING

Wire `ui.zig` into executor startup and lifecycle messages.

### 5.1 `executor/main.zig` startup

**Dimensions:**
- 5.1.1 PENDING Socket bind success: `✔ executor serving socket=/run/zombie/executor.sock`
- 5.1.2 PENDING Capability detection: `ℹ landlock=true cgroups_v2=true` or `▲ non-linux: degraded backend`
- 5.1.3 PENDING Bind failure: `ui.printError(...)` with structured error block

### 5.2 `executor/handler.zig` session lifecycle

**Dimensions:**
- 5.2.1 PENDING Session created: `ℹ session created execution_id=...`
- 5.2.2 PENDING Stage result: `✔ stage done` / `✖ stage failed` with color

### 5.3 `executor/runner.zig` execution

**Dimensions:**
- 5.3.1 PENDING Runner success: `✔ runner done tokens=N wall_seconds=N`
- 5.3.2 PENDING Runner failure: `ui.printError(...)` with error code and hint

### 5.4 `executor/cgroup.zig` and `executor/lease.zig`

**Dimensions:**
- 5.4.1 PENDING Cgroup created: `ℹ cgroup created memory_mb=N cpu_pct=N`
- 5.4.2 PENDING Lease reaped: `▲ reaped N expired sessions`

---

## 6.0 Build Integration

**Status:** PENDING

**Dimensions:**
- 6.1 PENDING `build.zig` adds `ui.zig` as a module dependency for both `zombied` and `zombied-executor` targets
- 6.2 PENDING Cross-compile verification: `make build` succeeds for x86_64-linux, aarch64-linux, aarch64-macos with the new module
- 6.3 PENDING `make lint` passes (line limit, greptile patterns, no hardcoded roles)

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 `src/ui.zig` exists, is under 200 lines, has zero external dependencies
- [ ] 7.2 Both `zombied` and `zombied-executor` compile with `ui.zig` linked on all 3 targets
- [ ] 7.3 `zombied migrate` shows spinner during connect, icons on retry/success/failure
- [ ] 7.4 `zombied doctor` shows ✔/✖ per check with color
- [ ] 7.5 `zombied-executor` startup shows icons for bind, capabilities, and errors
- [ ] 7.6 When `NO_COLOR=1` is set, all output is plain text (no ANSI escapes)
- [ ] 7.7 When stderr is not a TTY (piped), spinner is disabled and icons render without color
- [ ] 7.8 Structured log lines (`ts_ms=...`) are completely unchanged — grep for the pattern to verify
- [ ] 7.9 `make lint` and cross-compile pass clean

---

## 8.0 Out of Scope

- Progress bars — spinners cover all waiting cases
- Tables — zombied output is simple enough for formatted lines
- Color in structured log lines — machine output stays plain
- Custom log format for daemons — structured logs are correct for Loki
- Rewriting zombiectl's JS UI layer — it's already polished
