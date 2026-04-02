import { streamFetch, authHeaders } from "./http.js";
import { executeTool } from "./tool-executors.js";

const MAX_TOOL_CALLS = 10;
const MAX_WALL_MS = 30000;

const TOOL_DEFINITIONS = [
  {
    name: "read_file",
    description: "Read a file from the user's repo",
    input_schema: { type: "object", properties: { path: { type: "string" } }, required: ["path"] },
  },
  {
    name: "list_dir",
    description: "List directory contents",
    input_schema: { type: "object", properties: { path: { type: "string" } }, required: ["path"] },
  },
  {
    name: "glob",
    description: "Find files matching a glob pattern",
    input_schema: { type: "object", properties: { pattern: { type: "string" } }, required: ["pattern"] },
  },
];

/**
 * Run the agent tool-call loop.
 * POST messages + tools → receive SSE → execute tool_use locally → POST again → repeat.
 *
 * @param {string} endpoint - API path (e.g., "/v1/workspaces/{id}/spec/template")
 * @param {string} userMessage - Initial user message
 * @param {string} repoRoot - Absolute path to repo root
 * @param {object} ctx - CLI context (apiUrl, token, etc.)
 * @param {object} callbacks - { onToolCall, onText, onDone, onError }
 * @returns {Promise<{text: string, usage: object|null, toolCalls: number, wallMs: number}>}
 */
export async function agentLoop(endpoint, userMessage, repoRoot, ctx, callbacks = {}) {
  const baseUrl = ctx.apiUrl;
  const headers = authHeaders({ token: ctx.token, apiKey: ctx.apiKey });
  const url = `${baseUrl}${endpoint}`;

  let messages = [{ role: "user", content: userMessage }];
  let toolCalls = 0;
  let accumulatedText = "";
  let lastUsage = null;
  const startTime = Date.now();

  while (toolCalls < MAX_TOOL_CALLS && (Date.now() - startTime) < MAX_WALL_MS) {
    const payload = { messages, tools: TOOL_DEFINITIONS };
    let pendingToolUses = [];
    let gotDone = false;

    await streamFetch(url, payload, headers, (event) => {
      switch (event.type) {
        case "tool_use":
          pendingToolUses.push(event.data);
          break;
        case "text_delta":
          if (event.data?.text) {
            accumulatedText += event.data.text;
            callbacks.onText?.(event.data.text);
          }
          break;
        case "done":
          lastUsage = event.data?.usage ?? null;
          gotDone = true;
          callbacks.onDone?.(event.data);
          break;
        case "error":
          callbacks.onError?.(event.data?.message ?? "unknown error");
          break;
      }
    }, { fetchImpl: ctx.fetchImpl, timeoutMs: MAX_WALL_MS - (Date.now() - startTime) });

    // If no tool calls, we're done
    if (pendingToolUses.length === 0 || gotDone) break;

    // Execute tool calls locally and build next message batch
    for (const tc of pendingToolUses) {
      toolCalls++;
      callbacks.onToolCall?.(tc);
      const result = executeTool(tc.name, tc.input, repoRoot);

      // Append assistant tool_use + user tool_result to message history
      messages.push({
        role: "assistant",
        content: JSON.stringify([{ type: "tool_use", id: tc.id, name: tc.name, input: tc.input }]),
      });
      messages.push({
        role: "user",
        content: JSON.stringify([{ type: "tool_result", tool_use_id: tc.id, content: result }]),
      });
    }

    if (toolCalls >= MAX_TOOL_CALLS) {
      callbacks.onError?.(`max tool calls reached (${MAX_TOOL_CALLS})`);
      break;
    }
  }

  const wallMs = Date.now() - startTime;
  if (wallMs >= MAX_WALL_MS && !accumulatedText) {
    callbacks.onError?.("wall time exceeded (30s)");
  }

  return { text: accumulatedText, usage: lastUsage, toolCalls, wallMs };
}
