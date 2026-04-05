// M22_001 §5: --watch SSE streaming for zombiectl run command.

const WATCH_MAX_RETRIES = 3;
const WATCH_RETRY_DELAY_MS = 2000;
const WATCH_CONNECT_TIMEOUT_MS = 30_000;

/**
 * Stream SSE events for a run in real time.
 *
 * DX contract:
 *   - Ctrl+C / SIGINT cancels the stream cleanly (exit 0, no orphaned connection)
 *   - Network errors retry up to WATCH_MAX_RETRIES with Last-Event-ID for gap-free replay
 *   - Progress: "waiting for events..." printed when no event arrives within 30s
 *   - Connection timeout after WATCH_CONNECT_TIMEOUT_MS
 */
async function streamRunWatch(ctx, runId, { apiHeaders: getHeaders, ui, writeLine }) {
  const url = `${ctx.apiUrl}/v1/runs/${encodeURIComponent(runId)}:stream`;
  const fetchFn = ctx.fetchImpl || globalThis.fetch;
  let lastEventId = null;
  let aborted = false;

  const abortController = new AbortController();
  const onSigint = () => {
    aborted = true;
    abortController.abort();
  };
  process.on("SIGINT", onSigint);

  writeLine(ctx.stderr, ui.dim("watch: streaming (Ctrl+C to stop)"));

  try {
    for (let attempt = 0; attempt <= WATCH_MAX_RETRIES; attempt++) {
      if (aborted) break;

      const reqHeaders = { ...getHeaders(ctx), Accept: "text/event-stream" };
      if (lastEventId) reqHeaders["Last-Event-ID"] = lastEventId;

      let response;
      try {
        response = await fetchFn(url, {
          method: "GET",
          headers: reqHeaders,
          signal: abortController.signal,
        });
      } catch (err) {
        if (aborted || err.name === "AbortError") break;
        if (attempt < WATCH_MAX_RETRIES) {
          writeLine(ctx.stderr, ui.dim(`watch: connection failed, retrying (${attempt + 1}/${WATCH_MAX_RETRIES})...`));
          await new Promise((r) => setTimeout(r, WATCH_RETRY_DELAY_MS));
          continue;
        }
        writeLine(ctx.stderr, ui.err(`watch: connection failed after ${WATCH_MAX_RETRIES} retries: ${err.message}`));
        return;
      }

      if (!response.ok) {
        writeLine(ctx.stderr, ui.err(`watch: stream returned ${response.status}`));
        if (response.status >= 500 && attempt < WATCH_MAX_RETRIES) {
          await new Promise((r) => setTimeout(r, WATCH_RETRY_DELAY_MS));
          continue;
        }
        return;
      }

      const result = await readSseStream(response, ctx, lastEventId, {
        ui, writeLine,
      });
      lastEventId = result.lastEventId;

      if (result.done || aborted) break;

      if (result.error && attempt < WATCH_MAX_RETRIES) {
        writeLine(ctx.stderr, ui.dim(`watch: stream interrupted, reconnecting (${attempt + 1}/${WATCH_MAX_RETRIES})...`));
        await new Promise((r) => setTimeout(r, WATCH_RETRY_DELAY_MS));
        continue;
      }

      if (result.error) {
        writeLine(ctx.stderr, ui.err(`watch: stream error: ${result.error.message}`));
      }
      break;
    }
  } finally {
    process.removeListener("SIGINT", onSigint);
  }
}

async function readSseStream(response, ctx, lastEventIdIn, { ui, writeLine }) {
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "message";
  let currentData = null;
  let lastActivity = Date.now();
  let heartbeatTimer = null;
  let lastEventId = lastEventIdIn;
  let streamDone = false;
  let streamError = null;

  const startHeartbeatCheck = () => {
    clearTimeout(heartbeatTimer);
    heartbeatTimer = setTimeout(() => {
      const idle = Math.round((Date.now() - lastActivity) / 1000);
      writeLine(ctx.stderr, ui.dim(`watch: waiting for events... (${idle}s idle)`));
      startHeartbeatCheck();
    }, WATCH_CONNECT_TIMEOUT_MS);
  };
  startHeartbeatCheck();

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      lastActivity = Date.now();
      startHeartbeatCheck();

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop();

      for (const raw of lines) {
        const line = raw.replace(/\r$/, "");
        if (line === "") {
          if (currentData !== null) {
            if (currentEvent === "run_complete") {
              writeLine(ctx.stdout, ui.ok(`run complete: ${currentData}`));
              streamDone = true;
              break;
            }
            if (currentEvent === "gate_result") {
              try {
                const d = JSON.parse(currentData);
                writeLine(ctx.stdout, `[${d.gate_name}] ${d.outcome} (loop ${d.loop}, ${d.wall_ms}ms)`);
              } catch { /* ignore malformed */ }
            }
            // M21_001 §1.4: interrupt acknowledgement from server
            if (currentEvent === "interrupt_ack") {
              try {
                const d = JSON.parse(currentData);
                writeLine(ctx.stdout, ui.dim(`→ steered (mode: ${d.mode})`));
              } catch { /* ignore malformed */ }
            }
          }
          currentEvent = "message";
          currentData = null;
          continue;
        }
        if (line.startsWith("data: ")) currentData = line.slice(6);
        else if (line.startsWith("event: ")) currentEvent = line.slice(7);
        else if (line.startsWith("id: ")) lastEventId = line.slice(4);
      }

      if (streamDone) break;
    }
  } catch (err) {
    if (err.name !== "AbortError") streamError = err;
  } finally {
    clearTimeout(heartbeatTimer);
    reader.cancel().catch(() => {});
  }

  return { done: streamDone, error: streamError, lastEventId };
}

export { streamRunWatch, WATCH_MAX_RETRIES, WATCH_RETRY_DELAY_MS, WATCH_CONNECT_TIMEOUT_MS };
