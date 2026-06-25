//! NullClaw → `ActivityFrame` adapter.
//!
//! Bridges the NullClaw `Observer` vtable + per-token `StreamCallback` into the
//! `contract.activity.ActivityFrame` shape the live tail consumes. NullClaw
//! fires these synchronously on the same thread that called `fleet.runSingle`,
//! so writes to the pipe are sequential — no locking, no concurrent producers.
//!
//! Args redaction: secret bytes are replaced with the canonical
//! `${secrets.NAME.FIELD}` placeholder before any frame leaves this child. The
//! redactor is fed api_key ∪ every `secrets_map` leaf (== the set
//! `secret_substitution` resolves), built in runner.execute via `collectSecrets`;
//! on redaction OOM the frame is DROPPED, never emitted raw (M100 §1).

const std = @import("std");
const logging = @import("log");
const Allocator = std.mem.Allocator;
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;
const providers = nullclaw.providers;
const contract = @import("contract");
const pipe_proto = @import("../pipe_proto.zig");
const inrun_memory = @import("inrun_memory.zig");
const client_errors = @import("client_errors.zig");
const stream_redactor = @import("stream_redactor.zig");

const ActivityFrame = contract.activity.ActivityFrame;

/// Live-tail progress sink: serializes each `ActivityFrame` and writes it as an
/// `activity` frame on the child's stdout (`pipe_proto`), where the parent reads
/// and forwards it to the control plane. Best-effort by contract — activity is
/// cosmetic, so encode/write failures drop the frame silently and never abort
/// the run (the durable record is `report`).
pub const ProgressWriter = struct {
    fd: std.posix.fd_t,
    alloc: Allocator,

    pub fn write(self: *const ProgressWriter, frame: ActivityFrame) void {
        const json = std.json.Stringify.valueAlloc(self.alloc, frame, .{}) catch return;
        defer self.alloc.free(json);
        pipe_proto.writeFrame(self.fd, .activity, json) catch |err| log.warn("activity_frame_write_failed", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
    }
};

const log = logging.scoped(.runner_progress);

/// One known secret value + the placeholder string the runner will
/// substitute when it appears in a tool argument. Caller-owned;
/// borrowed for the lifetime of the Adapter.
pub const Secret = struct {
    /// The literal byte sequence to redact (e.g. the resolved api_key).
    value: []const u8,
    /// `${secrets.NAME.FIELD}` placeholder, e.g. `${secrets.llm.api_key}`.
    placeholder: []const u8,
};

/// Adapter object that holds the per-call writer, allocator, and the
/// list of secrets to redact. Must outlive the fleet run; runner.zig
/// stack-allocates one for the duration of `fleet.runSingle`.
pub const Adapter = struct {
    writer: *const ProgressWriter,
    alloc: Allocator,
    secrets: []const Secret,
    /// L1 context-lifecycle: every Nth completed tool call, emit a
    /// structured log line so observers can confirm the fleet's
    /// SKILL.md "snapshot every N tools" cadence is being prompted.
    /// 0 disables the cadence (tests + non-streaming paths).
    memory_checkpoint_every: u32 = 0,
    /// Flushes the in-run store to the parent on each checkpoint-due tick (and
    /// the engine calls it once more at run end). Null in non-streaming/test
    /// paths or when the in-run store could not be built — capture is then a
    /// no-op, never a hard error. Borrowed; the engine owns its lifetime.
    memory_capturer: ?*inrun_memory.MemoryCapturer = null,
    /// Source of the cumulative split accessors for `usage` frames; set by
    /// runner.zig after fleet init (null in tests/non-streaming → emit no-ops).
    /// Borrowed for the run; NullClaw fires the metric on the run thread.
    fleet: ?*const nullclaw.agent.Agent = null,
    /// L2 context-lifecycle: once the cumulative completed-tool count
    /// crosses this threshold, log a `tool_window_exceeded` line each
    /// subsequent call. SKILL.md prose tells the fleet to compact
    /// findings via memory_store + memory_forget once the threshold
    /// is hit; the runtime log lets on-call confirm the prompt
    /// landed. 0 disables. NullClaw doesn't expose mid-loop
    /// conversation mutation, so the fleet — not the runtime —
    /// drops oldest tool results by consolidating into memory.
    tool_window: u32 = 0,
    /// L3 context-lifecycle: fraction of the model's context cap (0.0..1.0)
    /// at which the fleet is told to snapshot + chunk. Computed at every
    /// `llm_response` observer event against `prompt_tokens /
    /// context_cap_tokens`. Crossing the threshold logs a structured
    /// `chunk_threshold_breached` line and bumps `chunk_threshold_logs`.
    /// 0.0 disables the layer. NullClaw doesn't expose mid-loop
    /// interrupt, so the runtime cannot force a chunk — the fleet does
    /// it via SKILL prose ("when context fills, return content='needs
    /// continuation'"). Spec §6 wires this gap as observability + prose.
    stage_chunk_threshold: f32 = 0.0,
    /// L3 context-lifecycle: hard cap on input tokens for the resolved
    /// model (M49 install-skill writes this into frontmatter; M48 self-managed
    /// writes from the credential body). Required denominator for the
    /// fill ratio — when 0, L3 short-circuits (no ratio, no log).
    context_cap_tokens: u32 = 0,
    /// Mutable counters — bumped by the observer thread (NullClaw fires
    /// events on the same thread that called `fleet.runSingle`, so no
    /// atomics needed).
    tool_call_count: u32 = 0,
    nudges_emitted: u32 = 0,
    window_exceeded_logs: u32 = 0,
    chunk_threshold_logs: u32 = 0,
    last_prompt_tokens: u32 = 0,
    /// Cross-delta carry for streaming redaction: holds the raw un-emitted tail
    /// (a possible secret head) between `streamCallbackThunk` calls so a secret
    /// split across two chunks is still redacted (M100 §1). Owned by this
    /// adapter; released by `deinit`.
    redact_carry: std.ArrayListUnmanaged(u8) = .empty,

    /// NullClaw Observer view of this adapter. Pass to `Fleet.fromConfig`.
    pub fn observer(self: *Adapter) observability.Observer {
        return .{
            .ptr = self,
            .vtable = &observer_vtable,
        };
    }

    /// `(stream_callback, stream_ctx)` pair to set on the Fleet.
    pub fn streamCallback(self: *Adapter) struct {
        cb: providers.StreamCallback,
        ctx: *anyopaque,
    } {
        return .{ .cb = streamCallbackThunk, .ctx = self };
    }

    /// Write one cumulative `usage` frame from the fleet's split accessors.
    /// Fires on every tokens_used metric (each turn/summary fold — the fleet
    /// folds before it emits the metric) and once more at run end from
    /// runner.zig; best-effort like every progress frame.
    pub fn emitUsage(self: *Adapter) void {
        const fleet = self.fleet orelse return;
        const snap = pipe_proto.UsageSnapshot{
            .input_tokens = fleet.promptTokensUsed(),
            .output_tokens = fleet.completionTokensUsed(),
        };
        const payload = snap.encode();
        pipe_proto.writeFrame(self.writer.fd, .usage, &payload) catch |err|
            log.warn("usage_frame_write_failed", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
    }

    /// Release the streaming-redaction carry. Call once when the run ends; the
    /// held tail (a partial-secret head, if any) is intentionally never emitted
    /// — the redacted final reply carries the complete content.
    pub fn deinit(self: *Adapter, alloc: Allocator) void {
        self.redact_carry.deinit(alloc);
    }

    fn fromPtr(ptr: *anyopaque) *Adapter {
        return @ptrCast(@alignCast(ptr));
    }
};

// ── Observer vtable ─────────────────────────────────────────────────────────

const observer_vtable: observability.Observer.VTable = .{
    .record_event = observerRecordEvent,
    .record_metric = observerRecordMetric,
    .flush = observerFlush,
    .name = observerName,
    .get_trace_id = observerGetTraceId,
    .set_trace_id = observerSetTraceId,
};

fn observerRecordEvent(ptr: *anyopaque, event: *const observability.ObserverEvent) void {
    const self = Adapter.fromPtr(ptr);
    switch (event.*) {
        .tool_call_start => |b| {
            // NullClaw exposes the tool name at start but not the args —
            // emit `tool_call_started` with an empty redacted args object
            // so live tail shows the tool kicking off; the args appear
            // in the durable record (fleet_events.request_json) and in
            // the `tool_call_completed` follow-up frame's UI affordances.
            const frame = ActivityFrame{
                .tool_call_started = .{ .name = b.tool, .args_redacted = "{}" },
            };
            self.writer.write(frame);
        },
        .tool_call => |b| {
            // Best-effort: if NullClaw passed the args blob, redact it
            // and emit a fresh `tool_call_started` carrying the
            // post-redaction bytes; otherwise just close the call.
            if (b.args) |raw| {
                // Drop the args frame on redaction OOM (raw could carry a secret);
                // the completed frame below still closes the call (M100 §1).
                if (redactBytes(self.alloc, raw, self.secrets)) |redacted| {
                    defer if (redacted.ptr != raw.ptr) self.alloc.free(redacted);
                    self.writer.write(.{ .tool_call_started = .{ .name = b.tool, .args_redacted = redacted } });
                } else |err| {
                    log.warn("tool_args_redaction_failed_frame_dropped", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
                }
            }
            const ms_signed = std.math.cast(i64, b.duration_ms) orelse std.math.maxInt(i64);
            const done_frame = ActivityFrame{
                .tool_call_completed = .{ .name = b.tool, .ms = ms_signed },
            };
            self.writer.write(done_frame);
            self.tool_call_count += 1;
            // L1 nudge: SKILL.md prose tells the fleet to snapshot via
            // memory_store on this cadence. The runtime side just logs
            // the threshold hit so on-call can confirm the prompt is
            // landing — actual prompt engineering is in the skill.
            if (self.memory_checkpoint_every > 0 and
                self.tool_call_count % self.memory_checkpoint_every == 0)
            {
                self.nudges_emitted += 1;
                log.debug("memory_checkpoint_due", .{
                    .tool_count = self.tool_call_count,
                    .every = self.memory_checkpoint_every,
                    .nudges_emitted = self.nudges_emitted,
                });
                // Flush the in-run store to the parent so a long run's learned
                // memory is durable before it finishes (run-end is not the only
                // capture point). Best-effort — a blip never disturbs the run.
                if (self.memory_capturer) |c| c.capture();
            }
            // L2 window: once the cumulative count crosses the window
            // threshold, every subsequent call emits a structured line
            // so on-call can spot a runaway incident in the activity
            // stream. The fleet compacts findings via memory_store at
            // the SKILL prose's direction — the runtime doesn't drop
            // anything from the conversation itself.
            if (self.tool_window > 0 and self.tool_call_count > self.tool_window) {
                self.window_exceeded_logs += 1;
                log.debug("tool_window_exceeded", .{
                    .tool_count = self.tool_call_count,
                    .window = self.tool_window,
                    .excess = self.tool_call_count - self.tool_window,
                });
            }
        },
        .llm_response => |b| {
            // L3 chunk threshold: NullClaw doesn't expose mid-loop
            // interrupt, so we observe instead of force. After every
            // LLM round-trip, compute fill = prompt_tokens / cap. When
            // it crosses the configured threshold, emit a structured
            // log. SKILL.md prose tells the fleet to write a snapshot
            // and end with content='needs continuation' on the next
            // turn — the runtime can see the prompt tokens but not
            // halt the loop, so the fleet owns the chunk decision.
            const prompt: u32 = b.prompt_tokens orelse 0;
            self.last_prompt_tokens = prompt;
            if (self.stage_chunk_threshold > 0.0 and self.context_cap_tokens > 0 and prompt > 0) {
                const ratio: f32 = @as(f32, @floatFromInt(prompt)) / @as(f32, @floatFromInt(self.context_cap_tokens));
                if (ratio >= self.stage_chunk_threshold) {
                    self.chunk_threshold_logs += 1;
                    log.debug("chunk_threshold_breached", .{
                        .prompt_tokens = prompt,
                        .cap_tokens = self.context_cap_tokens,
                        .ratio = ratio,
                        .threshold = self.stage_chunk_threshold,
                        .logs_emitted = self.chunk_threshold_logs,
                    });
                }
            }
        },
        else => {}, // fleet_start / fleet_end / llm_request / heartbeat / etc. are not on the substrate
    }
}

fn observerRecordMetric(ptr: *anyopaque, metric: *const observability.ObserverMetric) void {
    switch (metric.*) {
        .tokens_used => Adapter.fromPtr(ptr).emitUsage(),
        else => {},
    }
}
fn observerFlush(_: *anyopaque) void {}
fn observerName(_: *anyopaque) []const u8 {
    return "agentsfleet-runner-progress";
}
fn observerGetTraceId(_: *anyopaque) ?[32]u8 {
    return null;
}
fn observerSetTraceId(_: *anyopaque, _: [32]u8) void {}

// ── Stream callback thunk ───────────────────────────────────────────────────

fn streamCallbackThunk(ctx: *anyopaque, chunk: providers.StreamChunk) void {
    const self = Adapter.fromPtr(ctx);
    if (chunk.is_final) return; // held tail is dropped; the final reply carries it
    if (chunk.delta.len == 0) return;
    // Redact across chunk boundaries: a secret split between two deltas must not
    // leak. `push` holds back a partial-secret head and emits only safe bytes;
    // on OOM it leaves the carry intact and we drop the chunk (M100 §1).
    const emit = stream_redactor.push(self.alloc, &self.redact_carry, chunk.delta, self.secrets) catch |err| {
        log.warn("chunk_redaction_failed_dropped", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
        return;
    };
    defer self.alloc.free(emit);
    if (emit.len == 0) return;
    self.writer.write(.{ .fleet_response_chunk = .{ .text = emit } });
}

// ── Args redaction ──────────────────────────────────────────────────────────

/// Walk `raw` and replace every occurrence of each secret value with its
/// placeholder. Returns the original slice unchanged when no secret
/// matched (caller checks `result.ptr == raw.ptr` to skip a free). The
/// returned bytes may not be valid JSON — preserving the secret leak
/// boundary takes precedence over JSON well-formedness on the live
/// tail; the durable record already redacts upstream.
pub fn redactBytes(alloc: Allocator, raw: []const u8, secrets: []const Secret) ![]const u8 {
    if (secrets.len == 0) return raw;
    var any_hit = false;
    for (secrets) |s| {
        if (s.value.len == 0) continue;
        if (std.mem.indexOf(u8, raw, s.value) != null) {
            any_hit = true;
            break;
        }
    }
    if (!any_hit) return raw;

    var current = try alloc.dupe(u8, raw);
    errdefer alloc.free(current);
    for (secrets) |s| {
        if (s.value.len == 0) continue;
        if (std.mem.indexOf(u8, current, s.value) == null) continue;
        const replaced_count = std.mem.replacementSize(u8, current, s.value, s.placeholder);
        const replaced = try alloc.alloc(u8, replaced_count);
        _ = std.mem.replace(u8, current, s.value, s.placeholder, replaced);
        alloc.free(current);
        current = replaced;
    }
    return current;
}

// redactBytes contract tests live in runner_progress_redact_test.zig; the
// streaming cross-chunk redactor and its tests live in stream_redactor.zig.
