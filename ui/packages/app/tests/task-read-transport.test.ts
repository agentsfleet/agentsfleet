// @vitest-environment node
import { createServer } from "node:http";
import { once } from "node:events";
import { afterEach, describe, expect, it, vi } from "vitest";
import { readTask, readTasks } from "@/lib/api/tasks";
import { TASK_IDS, detail, summary } from "./helpers/task-reads";

afterEach(() => {vi.unstubAllGlobals(); vi.restoreAllMocks();});

describe("task reads at the browser HTTP boundary", () => {
  it("encodes scope, preserves native quantities and refuses incomplete or malformed state", async () => {
    const task = summary();
    const expected = detail(task);
    const network = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(Response.json({items: [task], next_cursor: null}))
      .mockResolvedValueOnce(Response.json(expected))
      .mockResolvedValueOnce(Response.json({items: [task], next_cursor: "more"}))
      .mockResolvedValueOnce(Response.json({...expected, context: {...expected.context, questions: []}}))
      .mockResolvedValueOnce(Response.json({...expected, context: {...expected.context, usage: {...expected.context.usage, model_nanos: 12000000}}}));
    vi.stubGlobal("fetch", network);
    const signal = new AbortController().signal;
    expect(await readTasks("workspace/one", "fleet?two", signal)).toEqual({kind: "ready", value: {items: [task], next_cursor: null}});
    expect(await readTask("workspace/one", "fleet?two", TASK_IDS[0], signal)).toEqual({kind: "ready", value: expected});
    const calls = network.mock.calls;
    expect(calls.map(([path]) => path)).toEqual([
      "/live/v1/workspaces/workspace%2Fone/fleets/fleet%3Ftwo/tasks?limit=100",
      `/live/v1/workspaces/workspace%2Fone/fleets/fleet%3Ftwo/tasks/${TASK_IDS[0]}`,
    ]);
    for (const [, options] of calls) expect(options).toMatchObject({cache: "no-store", redirect: "manual", signal: expect.any(AbortSignal)});
    expect(await readTasks("w", "f", signal)).toEqual({kind: "failed", reason: "invalid"});
    for (let attempt = 0; attempt < 2; attempt += 1) expect(await readTask("w", "f", TASK_IDS[0], signal)).toEqual({kind: "failed", reason: "invalid"});
    expect(network).toHaveBeenCalledTimes(5);
  });

  it("keeps authentication, absence and uncertain transport failures distinct without retries", async () => {
    const redirect = new Response(null);
    Object.defineProperty(redirect, "type", {value: "opaqueredirect"});
    const network = vi.fn<typeof fetch>();
    for (const status of [401, 403, 404, 503]) network.mockResolvedValueOnce(new Response(null, {status}));
    network.mockResolvedValueOnce(redirect)
      .mockRejectedValueOnce(new TypeError("network unavailable"))
      .mockResolvedValueOnce(new Response("{not json"));
    vi.stubGlobal("fetch", network);
    const signal = new AbortController().signal;
    const outcomes = [];
    for (let attempt = 0; attempt < 7; attempt += 1) outcomes.push(await readTasks("w", "f", signal));
    expect(outcomes).toEqual(["access", "access", "missing", "unreachable", "access", "unreachable", "invalid"].map(reason => ({kind: "failed", reason})));
    expect(network).toHaveBeenCalledTimes(7);
  });

  it("bounds header and body waits with the same deadline and propagates caller cancellation", async () => {
    const nativeFetch = (globalThis as {__realFetch?: typeof fetch}).__realFetch!;
    const deadlineFactory = vi.spyOn(AbortSignal, "timeout");
    let mode: "headers" | "body" | "caller" = "headers";
    let received = 0;
    let headers = 0;
    let closed = 0;
    const callers: AbortController[] = [];
    const reads: Promise<unknown>[] = [];
    const server = createServer((_request, response) => {
      received += 1;
      response.once("close", () => {closed += 1;});
      if (mode === "body") {
        response.writeHead(200, {"Content-Type": "application/json"});
        response.write('{"items":[');
      }
      // Leave headers or the incomplete JSON body pending until the caller closes it.
    });
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("Task transport listener missing");
    const origin = `http://127.0.0.1:${address.port}`;
    const network = vi.fn<typeof fetch>().mockImplementation(async (path, options) => {
      if (typeof path !== "string") throw new Error("Task reads must supply their same-origin path");
      const response = await nativeFetch(new URL(path, origin), options);
      headers += 1;
      return response;
    });
    vi.stubGlobal("fetch", network);
    try {
      for (const [index, boundary] of (["headers", "body", "caller"] as const).entries()) {
        mode = boundary;
        const deadline = new AbortController();
        const caller = new AbortController();
        callers.push(caller);
        deadlineFactory.mockReturnValueOnce(deadline.signal);
        let completed: unknown;
        const reading = readTasks("w", "f", caller.signal).then(result => {completed = result;});
        reads.push(reading);
        await vi.waitFor(() => expect(received).toBe(index + 1), {timeout: 5_000});
        if (boundary === "body") await vi.waitFor(() => expect(headers).toBe(1), {timeout: 5_000});
        expect(completed).toBeUndefined();
        if (boundary === "caller") caller.abort();
        else deadline.abort(new DOMException("deadline", "TimeoutError"));
        await vi.waitFor(() => expect(completed).toEqual({kind: "failed", reason: boundary === "caller" ? "unreachable" : "timeout"}), {timeout: 5_000});
        await vi.waitFor(() => expect(closed).toBe(index + 1), {timeout: 5_000});
        expect(network).toHaveBeenCalledTimes(index + 1);
      }
      expect([received, headers, closed]).toEqual([3, 1, 3]);
    } finally {
      for (const caller of callers) caller.abort();
      server.closeAllConnections();
      const results = await Promise.allSettled(reads);
      await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
      expect(results.every(result => result.status === "fulfilled")).toBe(true);
    }
  });
});
