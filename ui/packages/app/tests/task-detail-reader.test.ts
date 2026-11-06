import { currentTaskRef } from "@/components/domain/chat/task-references";
import { afterEach, expect, it, vi } from "vitest";
import { loadTaskDetails } from "@/components/domain/chat/task-detail-reader";
import { detail, summary, TASK_IDS } from "./helpers/task-reads";
afterEach(() => {vi.unstubAllGlobals(); vi.useRealTimers();});

it("bounds requests, shares each requested identity once, and aborts remaining work", async () => {
  const tasks = [...TASK_IDS.map(id => summary(id)), summary("019a42b0-0000-7000-8000-000000000126")];
  const requests = tasks.map(() => Promise.withResolvers<Response>());
  const network = vi.fn<typeof fetch>().mockImplementation(() => requests[network.mock.calls.length - 1]!.promise);
  vi.stubGlobal("fetch", network);
  const controller = new AbortController();
  const observe = vi.fn();
  const loading = loadTaskDetails("workspace", "fleet", tasks.map(currentTaskRef), controller.signal, observe);
  expect(network).toHaveBeenCalledTimes(3);
  requests[0]!.resolve(Response.json(detail(tasks[0])));
  await vi.waitFor(() => expect(network).toHaveBeenCalledTimes(4));
  expect(observe).toHaveBeenCalledWith(currentTaskRef(tasks[0]!), {kind: "ready", value: detail(tasks[0])});
  controller.abort();
  requests.slice(1).forEach((request, index) => request.resolve(Response.json(detail(tasks[index + 1]))));
  await loading;
  expect(observe.mock.calls.filter(([, result]) => result !== null)).toHaveLength(1);
  for (const [, init] of network.mock.calls) expect(init?.signal?.aborted).toBe(true);
  await loadTaskDetails("workspace", "fleet", tasks.map(currentTaskRef), controller.signal, observe);
  expect(network).toHaveBeenCalledTimes(4);
});

it("refuses mismatched task identity and older versions, then recovers transient responses automatically", async () => {
  vi.useFakeTimers();
  const task = summary(TASK_IDS[0], "running", 2);
  const network = vi.fn<typeof fetch>().mockResolvedValueOnce(Response.json(detail(summary(TASK_IDS[1], "running", 2))))
    .mockResolvedValueOnce(Response.json(detail(summary(TASK_IDS[0], "running", 1))))
    .mockResolvedValueOnce(new Response(null, {status: 503})).mockResolvedValueOnce(Response.json(detail(task)));
  vi.stubGlobal("fetch", network);
  const observe = vi.fn();
  const controller = new AbortController();
  const loading = loadTaskDetails("workspace", "fleet", [currentTaskRef(task)], controller.signal, observe);
  await vi.advanceTimersByTimeAsync(0);
  expect(observe).toHaveBeenLastCalledWith(currentTaskRef(task), {kind: "failed", reason: "invalid"});
  await vi.advanceTimersByTimeAsync(5000);
  expect(observe).toHaveBeenLastCalledWith(currentTaskRef(task), {kind: "failed", reason: "invalid"});
  await vi.advanceTimersByTimeAsync(5000);
  expect(observe).toHaveBeenLastCalledWith(currentTaskRef(task), {kind: "failed", reason: "unreachable"});
  await vi.advanceTimersByTimeAsync(5000);
  await loading;
  expect(observe).toHaveBeenLastCalledWith(currentTaskRef(task), {kind: "ready", value: detail(task)});
  expect(network).toHaveBeenCalledTimes(4);
});

it("does not retry access or missing responses and cancels pending recovery on scope disposal", async () => {
  vi.useFakeTimers();
  const network = vi.fn<typeof fetch>().mockResolvedValueOnce(new Response(null, {status: 403}))
    .mockResolvedValueOnce(new Response(null, {status: 404})).mockResolvedValueOnce(new Response(null, {status: 503}));
  vi.stubGlobal("fetch", network);
  const controller = new AbortController();
  const observe = vi.fn();
  await loadTaskDetails("workspace", "fleet", TASK_IDS.slice(0, 2).map(id => currentTaskRef(summary(id))), controller.signal, observe);
  expect(observe).toHaveBeenCalledWith(currentTaskRef(summary(TASK_IDS[0])), {kind: "failed", reason: "access"});
  expect(observe).toHaveBeenCalledWith(currentTaskRef(summary(TASK_IDS[1])), {kind: "failed", reason: "missing"});
  const pending = loadTaskDetails("workspace", "fleet", [currentTaskRef(summary())], controller.signal, observe);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  await pending;
  await vi.advanceTimersByTimeAsync(5000);
  expect(network).toHaveBeenCalledTimes(3);
});
