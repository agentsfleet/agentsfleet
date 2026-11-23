import { afterEach, describe, expect, it, vi } from "vitest";

import { TOOL_CALL_READ, readToolCall } from "./fleet-tool-call-reader";

const AT = { workspaceId: "ws 1", fleetId: "flt_1", turnId: "evt_1", callId: "f1:3" };
const URL_FOR_AT = "/live/v1/workspaces/ws%201/fleets/flt_1/turns/evt_1/items/f1%3A3";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function answer(response: Response | Promise<Response>) {
  const fetchMock = vi.fn(() => Promise.resolve(response));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

describe("readToolCall", () => {
  it("should read one call in full, narrowing what crosses into the browser", async () => {
    const changes = [{ path: "a.md", added: 24, removed: 8 }];
    const fetchMock = answer(json({
      call_id: "f1:3", kind: "edit", tool: "file_edit", status: "succeeded",
      arguments: { path: "a.md" }, truncated_arguments: true, output: "ok", truncated: "yes", changes,
    }));
    const read = await readToolCall(AT, new AbortController().signal);
    expect(fetchMock).toHaveBeenCalledWith(URL_FOR_AT, expect.objectContaining({ redirect: "manual" }));
    // A malformed flag reads as not cut; the recorded words and changes cross whole.
    expect(read).toEqual({
      kind: TOOL_CALL_READ.FULL,
      call: { kind: "edit", tool: "file_edit", status: "succeeded", args: { path: "a.md" }, argsTruncated: true, output: "ok", outputTruncated: false, changes },
    });
  });

  it("should read a kind, tool or status it cannot narrow as absent", async () => {
    answer(json({ kind: "integration", tool: "", status: "running", output: "ok" }));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({
      kind: TOOL_CALL_READ.FULL,
      call: { args: undefined, argsTruncated: false, output: "ok", outputTruncated: false, changes: [] },
    });
  });

  it("should read arguments that are no object as none", async () => {
    answer(json({ arguments: [1], output: "" }));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({
      kind: TOOL_CALL_READ.FULL,
      call: { args: undefined, argsTruncated: false, output: "", outputTruncated: false, changes: [] },
    });
  });

  it("should tell a call kept without full output from a read that failed", async () => {
    answer(json({}, 404));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({ kind: TOOL_CALL_READ.NOT_KEPT });
    answer(json({}, 502));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({ kind: TOOL_CALL_READ.FAILED });
    // A body with no output is no call.
    answer(json({ call_id: "f1:3" }));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({ kind: TOOL_CALL_READ.FAILED });
    answer(new Response("not json", { status: 200 }));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({ kind: TOOL_CALL_READ.FAILED });
    // JSON that is no object is no call either.
    answer(json([{ output: "ok" }]));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({ kind: TOOL_CALL_READ.FAILED });
  });

  it("should fail, never throw, when the read is cancelled", async () => {
    vi.stubGlobal("fetch", vi.fn((_url: string, init: RequestInit) => new Promise((_, reject) => {
      init.signal?.addEventListener("abort", () => reject(init.signal?.reason));
    })));
    const cancelled = new AbortController();
    const pending = readToolCall(AT, cancelled.signal);
    cancelled.abort();
    expect(await pending).toEqual({ kind: TOOL_CALL_READ.FAILED });
  });
});
