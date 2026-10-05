import { afterEach, describe, expect, it, vi } from "vitest";

import { TOOL_CALL_READ, TOOL_CALL_READ_TIMEOUT_MS, readToolCall } from "./fleet-tool-call-reader";

const AT = { workspaceId: "ws 1", fleetId: "flt_1", eventId: "evt_1", callId: "f1:3" };
const URL_FOR_AT = "/live/v1/workspaces/ws%201/fleets/flt_1/events/evt_1/tool-calls/f1%3A3";

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
    const fetchMock = answer(json({ call_id: "f1:3", arguments: { path: "a.md" }, truncated_arguments: true, output: "ok", truncated: "yes" }));
    const read = await readToolCall(AT, new AbortController().signal);
    expect(fetchMock).toHaveBeenCalledWith(URL_FOR_AT, expect.objectContaining({ redirect: "manual" }));
    // A malformed flag reads as not cut.
    expect(read).toEqual({ kind: TOOL_CALL_READ.FULL, call: { args: { path: "a.md" }, argsTruncated: true, output: "ok", outputTruncated: false } });
  });

  it("should read arguments that are no object as none", async () => {
    answer(json({ arguments: [1], output: "" }));
    expect(await readToolCall(AT, new AbortController().signal)).toEqual({
      kind: TOOL_CALL_READ.FULL,
      call: { args: undefined, argsTruncated: false, output: "", outputTruncated: false },
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

  it("should fail when the read outlasts its timeout", async () => {
    // AbortSignal.timeout runs on the real clock, so the test owns the signal
    // it hands out instead of waiting the timeout out.
    const timer = new AbortController();
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout").mockReturnValue(timer.signal);
    vi.stubGlobal("fetch", vi.fn((_url: string, init: RequestInit) => new Promise((_, reject) => {
      init.signal?.addEventListener("abort", () => reject(init.signal?.reason));
    })));
    const hung = readToolCall(AT, new AbortController().signal);
    expect(timeoutSpy).toHaveBeenCalledWith(TOOL_CALL_READ_TIMEOUT_MS);
    timer.abort(new DOMException("timed out", "TimeoutError"));
    expect(await hung).toEqual({ kind: TOOL_CALL_READ.FAILED });
  });
});
