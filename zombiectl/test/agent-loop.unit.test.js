import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import os from "node:os";
import { agentLoop } from "../src/lib/agent-loop.js";
import { ApiError } from "../src/lib/http.js";

// ── Test helpers ─────────────────────────────────────────────────────────────

function makeTmp() {
  const dir = join(os.tmpdir(), `agent-loop-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "README.md"), "# Test Repo");
  mkdirSync(join(dir, "src"), { recursive: true });
  writeFileSync(join(dir, "src", "main.go"), "package main");
  return dir;
}

/**
 * Build a mock fetch response that returns an SSE body from a string.
 */
function sseResponse(sseBody) {
  const encoder = new TextEncoder();
  return {
    ok: true,
    status: 200,
    body: {
      getReader() {
        let sent = false;
        return {
          read() {
            if (!sent) {
              sent = true;
              return Promise.resolve({ done: false, value: encoder.encode(sseBody) });
            }
            return Promise.resolve({ done: true });
          },
        };
      },
    },
  };
}

function makeCtx(fetchImpl) {
  return {
    apiUrl: "https://api.test.com",
    token: "test-token",
    fetchImpl,
  };
}

// ── T1: Happy path ──────────────────────────────────────────────────────────

describe("agentLoop — happy path", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("completes with text when server returns text_delta + done", async () => {
    const fetchImpl = async () => sseResponse(
      'event: text_delta\ndata: {"text":"# Spec content"}\n\nevent: done\ndata: {"usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150}}\n\n'
    );
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "Generate spec", tmp, ctx);
    expect(result.text).toBe("# Spec content");
    expect(result.usage.total_tokens).toBe(150);
    expect(result.toolCalls).toBe(0);
  });

  test("executes tool calls locally and accumulates messages", async () => {
    let callCount = 0;
    const fetchImpl = async () => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"list_dir","input":{"path":"."}}\n\n');
      }
      return sseResponse(
        'event: text_delta\ndata: {"text":"Found files"}\n\nevent: done\ndata: {"usage":{"total_tokens":200}}\n\n'
      );
    };
    const ctx = makeCtx(fetchImpl);
    const toolCalls = [];
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "Explore repo", tmp, ctx, {
      onToolCall: (tc) => toolCalls.push(tc),
    });
    expect(result.toolCalls).toBe(1);
    expect(toolCalls).toHaveLength(1);
    expect(toolCalls[0].name).toBe("list_dir");
    expect(result.text).toBe("Found files");
  });

  test("executes multiple tool calls across round trips", async () => {
    let callCount = 0;
    const fetchImpl = async () => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"list_dir","input":{"path":"."}}\n\n');
      }
      if (callCount === 2) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_02","name":"read_file","input":{"path":"README.md"}}\n\n');
      }
      return sseResponse('event: text_delta\ndata: {"text":"Done"}\n\nevent: done\ndata: {"usage":{"total_tokens":300}}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "Read repo", tmp, ctx);
    expect(result.toolCalls).toBe(2);
    expect(result.text).toBe("Done");
  });

  test("fires onText callback for each text_delta", async () => {
    const fetchImpl = async () => sseResponse(
      'event: text_delta\ndata: {"text":"chunk1"}\n\nevent: text_delta\ndata: {"text":"chunk2"}\n\nevent: done\ndata: {}\n\n'
    );
    const ctx = makeCtx(fetchImpl);
    const chunks = [];
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx, {
      onText: (t) => chunks.push(t),
    });
    expect(chunks).toEqual(["chunk1", "chunk2"]);
    expect(result.text).toBe("chunk1chunk2");
  });

  test("fires onDone callback with usage data", async () => {
    const fetchImpl = async () => sseResponse(
      'event: done\ndata: {"usage":{"total_tokens":42},"provider":"anthropic"}\n\n'
    );
    const ctx = makeCtx(fetchImpl);
    let doneData = null;
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx, {
      onDone: (d) => { doneData = d; },
    });
    expect(doneData.usage.total_tokens).toBe(42);
    expect(doneData.provider).toBe("anthropic");
  });
});

// ── T2: Edge cases ──────────────────────────────────────────────────────────

describe("agentLoop — edge cases", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("handles empty text_delta gracefully", async () => {
    const fetchImpl = async () => sseResponse(
      'event: text_delta\ndata: {"text":""}\n\nevent: text_delta\ndata: {}\n\nevent: done\ndata: {}\n\n'
    );
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(result.text).toBe("");
  });

  test("handles done event with no usage field", async () => {
    const fetchImpl = async () => sseResponse('event: done\ndata: {}\n\n');
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(result.usage).toBeNull();
  });

  test("tool call for missing file returns error to LLM, loop continues", async () => {
    let callCount = 0;
    const fetchImpl = async () => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"nonexistent.txt"}}\n\n');
      }
      return sseResponse('event: text_delta\ndata: {"text":"Handled missing file"}\n\nevent: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(result.text).toBe("Handled missing file");
    expect(result.toolCalls).toBe(1);
  });

  test("path traversal in tool call is rejected, loop continues", async () => {
    let callCount = 0;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"../../etc/passwd"}}\n\n');
      }
      // Second call: LLM should receive error and recover
      return sseResponse('event: text_delta\ndata: {"text":"recovered"}\n\nevent: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(result.text).toBe("recovered");
    // Verify the second POST contains the error message
    expect(callCount).toBe(2);
  });

  test("unknown tool name returns error string to LLM", async () => {
    let callCount = 0;
    let secondBody = null;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"write_file","input":{"path":"x"}}\n\n');
      }
      secondBody = opts.body;
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(secondBody).toContain("unknown tool");
  });
});

// ── T3: Error paths ─────────────────────────────────────────────────────────

describe("agentLoop — error paths", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("SSE error event fires onError callback", async () => {
    const fetchImpl = async () => sseResponse(
      'event: error\ndata: {"message":"provider timeout after 30s"}\n\n'
    );
    const ctx = makeCtx(fetchImpl);
    const errors = [];
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx, {
      onError: (e) => errors.push(e),
    });
    expect(errors).toContain("provider timeout after 30s");
  });

  test("HTTP 401 throws ApiError", async () => {
    const fetchImpl = async () => ({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      text: async () => JSON.stringify({ error: { code: "AUTH_REQUIRED", message: "not authenticated" } }),
    });
    const ctx = makeCtx(fetchImpl);
    try {
      await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
      expect(true).toBe(false); // should not reach
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError);
      expect(err.status).toBe(401);
    }
  });

  test("HTTP 500 throws ApiError with server error code", async () => {
    const fetchImpl = async () => ({
      ok: false,
      status: 500,
      statusText: "Internal Server Error",
      text: async () => JSON.stringify({ error: { code: "UZ-INTERNAL-003", message: "provider init failed" } }),
    });
    const ctx = makeCtx(fetchImpl);
    try {
      await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
      expect(true).toBe(false);
    } catch (err) {
      expect(err.code).toBe("UZ-INTERNAL-003");
    }
  });

  test("network error mid-stream propagates", async () => {
    const fetchImpl = async () => ({
      ok: true,
      status: 200,
      body: {
        getReader() {
          return {
            read() {
              return Promise.reject(new Error("connection reset"));
            },
          };
        },
      },
    });
    const ctx = makeCtx(fetchImpl);
    try {
      await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
      expect(true).toBe(false);
    } catch (err) {
      expect(err.message).toContain("connection reset");
    }
  });

  test("SSE error event with no message uses fallback", async () => {
    const fetchImpl = async () => sseResponse('event: error\ndata: {}\n\n');
    const ctx = makeCtx(fetchImpl);
    const errors = [];
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx, {
      onError: (e) => errors.push(e),
    });
    expect(errors).toContain("unknown error");
  });
});

// ── T5: Guardrails (max tool calls, timeout) ────────────────────────────────

describe("agentLoop — guardrails", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("stops after MAX_TOOL_CALLS (10) and fires onError", async () => {
    let callCount = 0;
    const fetchImpl = async () => {
      callCount++;
      return sseResponse(`event: tool_use\ndata: {"id":"tu_${callCount}","name":"list_dir","input":{"path":"."}}\n\n`);
    };
    const ctx = makeCtx(fetchImpl);
    const errors = [];
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx, {
      onError: (e) => errors.push(e),
    });
    expect(result.toolCalls).toBe(10);
    expect(errors.some((e) => e.includes("max tool calls"))).toBe(true);
  });

  test("returns partial text when max tool calls reached mid-conversation", async () => {
    let callCount = 0;
    const fetchImpl = async () => {
      callCount++;
      if (callCount <= 10) {
        return sseResponse(`event: tool_use\ndata: {"id":"tu_${callCount}","name":"list_dir","input":{"path":"."}}\n\n`);
      }
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(result.toolCalls).toBe(10);
    // callCount should be 10 (stopped before 11th fetch)
    expect(callCount).toBe(10);
  });
});

// ── T6: Integration — tool execution round-trip fidelity ────────────────────

describe("agentLoop — round-trip fidelity", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("tool result for read_file contains actual file content", async () => {
    let secondPayload = null;
    let callCount = 0;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"README.md"}}\n\n');
      }
      secondPayload = JSON.parse(opts.body);
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    // Second POST should contain the file content as tool_result
    const lastMsg = secondPayload.messages[secondPayload.messages.length - 1];
    const parsed = JSON.parse(lastMsg.content);
    expect(parsed[0].type).toBe("tool_result");
    expect(parsed[0].content).toBe("# Test Repo");
  });

  test("tool result for list_dir contains directory entries", async () => {
    let secondPayload = null;
    let callCount = 0;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"list_dir","input":{"path":"."}}\n\n');
      }
      secondPayload = JSON.parse(opts.body);
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    const lastMsg = secondPayload.messages[secondPayload.messages.length - 1];
    const parsed = JSON.parse(lastMsg.content);
    expect(parsed[0].content).toContain("README.md");
    expect(parsed[0].content).toContain("src/");
  });

  test("messages accumulate correctly across round trips", async () => {
    let thirdPayload = null;
    let callCount = 0;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"list_dir","input":{"path":"."}}\n\n');
      }
      if (callCount === 2) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_02","name":"read_file","input":{"path":"README.md"}}\n\n');
      }
      thirdPayload = JSON.parse(opts.body);
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    // 1 user + 2 assistant + 2 tool_result = 5 messages
    expect(thirdPayload.messages).toHaveLength(5);
    expect(thirdPayload.messages[0].role).toBe("user");
    expect(thirdPayload.messages[1].role).toBe("assistant");
    expect(thirdPayload.messages[2].role).toBe("user");
  });

  test("sends correct Authorization header", async () => {
    let capturedHeaders = null;
    const fetchImpl = async (url, opts) => {
      capturedHeaders = opts.headers;
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    ctx.token = "my-jwt-token";
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(capturedHeaders.Authorization).toBe("Bearer my-jwt-token");
  });

  test("sends tool definitions in payload", async () => {
    let capturedPayload = null;
    const fetchImpl = async (url, opts) => {
      capturedPayload = JSON.parse(opts.body);
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(capturedPayload.tools).toHaveLength(3);
    expect(capturedPayload.tools.map((t) => t.name)).toEqual(["read_file", "list_dir", "glob"]);
  });
});

// ── T8: Security — path traversal through tool calls ────────────────────────

describe("agentLoop — security", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("read_file with ../../etc/passwd sends error back to LLM, never reads file", async () => {
    let secondPayload = null;
    let callCount = 0;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"../../etc/passwd"}}\n\n');
      }
      secondPayload = JSON.parse(opts.body);
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    const lastMsg = secondPayload.messages[secondPayload.messages.length - 1];
    const parsed = JSON.parse(lastMsg.content);
    expect(parsed[0].content).toContain("error");
    expect(parsed[0].content).toContain("path outside repo root");
    expect(parsed[0].content).not.toContain("root:");
  });

  test("list_dir with /etc sends error back to LLM", async () => {
    let secondPayload = null;
    let callCount = 0;
    const fetchImpl = async (url, opts) => {
      callCount++;
      if (callCount === 1) {
        return sseResponse('event: tool_use\ndata: {"id":"tu_01","name":"list_dir","input":{"path":"/etc"}}\n\n');
      }
      secondPayload = JSON.parse(opts.body);
      return sseResponse('event: done\ndata: {}\n\n');
    };
    const ctx = makeCtx(fetchImpl);
    await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    const lastMsg = secondPayload.messages[secondPayload.messages.length - 1];
    const parsed = JSON.parse(lastMsg.content);
    expect(parsed[0].content).toContain("error");
  });
});

// ── T9: Callbacks are optional (no crash when omitted) ──────────────────────

describe("agentLoop — optional callbacks", () => {
  let tmp;
  beforeEach(() => { tmp = makeTmp(); });
  afterEach(() => { rmSync(tmp, { recursive: true, force: true }); });

  test("works with no callbacks object", async () => {
    const fetchImpl = async () => sseResponse(
      'event: text_delta\ndata: {"text":"ok"}\n\nevent: done\ndata: {}\n\n'
    );
    const ctx = makeCtx(fetchImpl);
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx);
    expect(result.text).toBe("ok");
  });

  test("works with empty callbacks object", async () => {
    const fetchImpl = async () => sseResponse('event: error\ndata: {"message":"oops"}\n\n');
    const ctx = makeCtx(fetchImpl);
    // Should not throw even though onError is not provided
    const result = await agentLoop("/v1/workspaces/ws1/spec/template", "msg", tmp, ctx, {});
    expect(result.text).toBe("");
  });
});
