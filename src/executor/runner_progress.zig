//! NullClaw → ProgressFrame adapter.
//!
//! Bridges the NullClaw `Observer` vtable + per-token `StreamCallback`
//! into the `ProgressFrame` shape the worker consumes via the streaming
//! StartStage RPC. NullClaw fires these synchronously on the same
//! thread that called `agent.runSingle`, so writes to the connection fd
//! are sequential — no locking, no concurrent producers.
//!
//! Args redaction: docs/architecture/ mandates that any secret bytes
//! substituted into tool arguments are replaced with the canonical
//! `${secrets.NAME.FIELD}` placeholder before the frame leaves the RPC
//! boundary. The redactor is constructed in runner.execute from the
//! known agent_config secrets (LLM api_key, GitHub token); runner.zig
//! is the only file that knows the secret VALUES, and they never
//! escape into a ProgressFrame.

const std = @import("std");
const logging = @import("log");
const Allocator = std.mem.Allocator;
const observability = @import("nullclaw").observability;
const providers = @import("nullclaw").providers;

const progress_writer_mod = @import("progress_writer.zig");
const progress_callbacks = @import("progress_callbacks.zig");

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
/// list of secrets to redact. Must outlive the agent run; runner.zig
/// stack-allocates one for the duration of `agent.runSingle`.
pub const Adapter = struct {
    writer: *const progress_writer_mod,
    alloc: Allocator,
    secrets: []const Secret,
    /// L1 context-lifecycle: every Nth completed tool call, emit a
    /// structured log line so observers can confirm the agent's
    /// SKILL.md "snapshot every N tools" cadence is being prompted.
    /// 0 disables the cadence (tests + non-streaming paths).
    memory_checkpoint_every: u32 = 0,
    /// L2 context-lifecycle: once the cumulative completed-tool count
    /// crosses this threshold, log a `tool_window_exceeded` line each
    /// subsequent call. SKILL.md prose tells the agent to compact
    /// findings via memory_store + memory_forget once the threshold
    /// is hit; the runtime log lets on-call confirm the prompt
    /// landed. 0 disables. NullClaw doesn't expose mid-loop
    /// conversation mutation, so the agent — not the runtime —
    /// drops oldest tool results by consolidating into memory.
    tool_window: u32 = 0,
    /// L3 context-lifecycle: fraction of the model's context cap (0.0..1.0)
    /// at which the agent is told to snapshot + chunk. Computed at every
    /// `llm_response` observer event against `prompt_tokens /
    /// context_cap_tokens`. Crossing the threshold logs a structured
    /// `chunk_threshold_breached` line and bumps `chunk_threshold_logs`.
    /// 0.0 disables the layer. NullClaw doesn't expose mid-loop
    /// interrupt, so the runtime cannot force a chunk — the agent does
    /// it via SKILL prose ("when context fills, return content='needs
    /// continuation'"). Spec §6 wires this gap as observability + prose.
    stage_chunk_threshold: f32 = 0.0,
    /// L3 context-lifecycle: hard cap on input tokens for the resolved
    /// model (M49 install-skill writes this into frontmatter; M48 BYOK
    /// writes from the credential body). Required denominator for the
    /// fill ratio — when 0, L3 short-circuits (no ratio, no log).
    context_cap_tokens: u32 = 0,
    /// Mutable counters — bumped by the observer thread (NullClaw fires
    /// events on the same thread that called `agent.runSingle`, so no
    /// atomics needed).
    tool_call_count: u32 = 0,
    nudges_emitted: u32 = 0,
    window_exceeded_logs: u32 = 0,
    chunk_threshold_logs: u32 = 0,
    last_prompt_tokens: u32 = 0,

    /// NullClaw Observer view of this adapter. Pass to `Agent.fromConfig`.
    pub fn observer(self: *Adapter) observability.Observer {
        return .{
            .ptr = self,
            .vtable = &observer_vtable,
        };
    }

    /// `(stream_callback, stream_ctx)` pair to set on the Agent.
    pub fn streamCallback(self: *Adapter) struct {
        cb: providers.StreamCallback,
        ctx: *anyopaque,
    } {
        return .{ .cb = streamCallbackThunk, .ctx = self };
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
            // in the durable record (zombie_events.request_json) and in
            // the `tool_call_completed` follow-up frame's UI affordances.
            const frame = progress_callbacks.ProgressFrame{
                .tool_call_started = .{ .name = b.tool, .args_redacted = "{}" },
            };
            self.writer.write(frame);
        },
        .tool_call => |b| {
            // Best-effort: if NullClaw passed the args blob, redact it
            // and emit a fresh `tool_call_started` carrying the
            // post-redaction bytes; otherwise just close the call.
            if (b.args) |raw| {
                const redacted = redactBytes(self.alloc, raw, self.secrets) catch raw;
                defer if (redacted.ptr != raw.ptr) self.alloc.free(redacted);
                const start_frame = progress_callbacks.ProgressFrame{
                    .tool_call_started = .{ .name = b.tool, .args_redacted = redacted },
                };
                self.writer.write(start_frame);
            }
            const ms_signed = std.math.cast(i64, b.duration_ms) orelse std.math.maxInt(i64);
            const done_frame = progress_callbacks.ProgressFrame{
                .tool_call_completed = .{ .name = b.tool, .ms = ms_signed },
            };
            self.writer.write(done_frame);
            self.tool_call_count += 1;
            // L1 nudge: SKILL.md prose tells the agent to snapshot via
            // memory_store on this cadence. The runtime side just logs
            // the threshold hit so on-call can confirm the prompt is
            // landing — actual prompt engineering is in the skill.
            if (self.memory_checkpoint_every > 0 and
                self.tool_call_count % self.memory_checkpoint_every == 0)
            {
                self.nudges_emitted += 1;
                log.info("memory_checkpoint_due", .{
                    .tool_count = self.tool_call_count,
                    .every = self.memory_checkpoint_every,
                    .nudges_emitted = self.nudges_emitted,
                });
            }
            // L2 window: once the cumulative count crosses the window
            // threshold, every subsequent call emits a structured line
            // so on-call can spot a runaway incident in the activity
            // stream. The agent compacts findings via memory_store at
            // the SKILL prose's direction — the runtime doesn't drop
            // anything from the conversation itself.
            if (self.tool_window > 0 and self.tool_call_count > self.tool_window) {
                self.window_exceeded_logs += 1;
                log.info("tool_window_exceeded", .{
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
            // log. SKILL.md prose tells the agent to write a snapshot
            // and end with content='needs continuation' on the next
            // turn — the runtime can see the prompt tokens but not
            // halt the loop, so the agent owns the chunk decision.
            const prompt: u32 = b.prompt_tokens orelse 0;
            self.last_prompt_tokens = prompt;
            if (self.stage_chunk_threshold > 0.0 and self.context_cap_tokens > 0 and prompt > 0) {
                const ratio: f32 = @as(f32, @floatFromInt(prompt)) / @as(f32, @floatFromInt(self.context_cap_tokens));
                if (ratio >= self.stage_chunk_threshold) {
                    self.chunk_threshold_logs += 1;
                    log.info("chunk_threshold_breached", .{
                        .prompt_tokens = prompt,
                        .cap_tokens = self.context_cap_tokens,
                        .ratio = ratio,
                        .threshold = self.stage_chunk_threshold,
                        .logs_emitted = self.chunk_threshold_logs,
                    });
                }
            }
        },
        else => {}, // agent_start / agent_end / llm_request / heartbeat / etc. are not on the substrate
    }
}

fn observerRecordMetric(_: *anyopaque, _: *const observability.ObserverMetric) void {}
fn observerFlush(_: *anyopaque) void {}
fn observerName(_: *anyopaque) []const u8 {
    return "zombie-runner-progress";
}
fn observerGetTraceId(_: *anyopaque) ?[32]u8 {
    return null;
}
fn observerSetTraceId(_: *anyopaque, _: [32]u8) void {}

// ── Stream callback thunk ───────────────────────────────────────────────────

fn streamCallbackThunk(ctx: *anyopaque, chunk: providers.StreamChunk) void {
    const self = Adapter.fromPtr(ctx);
    if (chunk.is_final) return;
    if (chunk.delta.len == 0) return;
    const redacted = redactBytes(self.alloc, chunk.delta, self.secrets) catch chunk.delta;
    defer if (redacted.ptr != chunk.delta.ptr) self.alloc.free(redacted);
    const frame = progress_callbacks.ProgressFrame{
        .agent_response_chunk = .{ .text = redacted },
    };
    self.writer.write(frame);
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

// ── Tests ───────────────────────────────────────────────────────────────────

test "redactBytes is a no-op when no secret value matches" {
    const alloc = std.testing.allocator;
    const secrets = [_]Secret{.{ .value = "shh-not-here", .placeholder = "${secrets.x.y}" }};
    const result = try redactBytes(alloc, "{\"cmd\":\"ls\"}", &secrets);
    try std.testing.expect(result.ptr == "{\"cmd\":\"ls\"}".ptr);
}

test "redactBytes replaces every occurrence of a secret value with the placeholder" {
    const alloc = std.testing.allocator;
    const secrets = [_]Secret{.{ .value = "sk-abc123", .placeholder = "${secrets.llm.api_key}" }};
    const result = try redactBytes(alloc, "{\"key\":\"sk-abc123\",\"again\":\"sk-abc123\"}", &secrets);
    defer alloc.free(result);
    try std.testing.expectEqualStrings(
        "{\"key\":\"${secrets.llm.api_key}\",\"again\":\"${secrets.llm.api_key}\"}",
        result,
    );
}

test "redactBytes handles multiple distinct secrets in one pass" {
    const alloc = std.testing.allocator;
    const secrets = [_]Secret{
        .{ .value = "sk-abc", .placeholder = "${secrets.llm.api_key}" },
        .{ .value = "ghp-xyz", .placeholder = "${secrets.github.token}" },
    };
    const result = try redactBytes(alloc, "{\"a\":\"sk-abc\",\"b\":\"ghp-xyz\"}", &secrets);
    defer alloc.free(result);
    try std.testing.expectEqualStrings(
        "{\"a\":\"${secrets.llm.api_key}\",\"b\":\"${secrets.github.token}\"}",
        result,
    );
}
