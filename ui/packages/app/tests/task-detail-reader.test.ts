import { currentDurableTaskRef } from "@/components/domain/tasks/task-references";
import { afterEach, expect, it, vi } from "vitest";
import { loadDurableTaskDetails } from "@/components/domain/tasks/task-detail-reader";
import { detail, summary, DURABLE_TASK_IDS } from "./helpers/task-reads";
afterEach(() => {vi.unstubAllGlobals(); vi.useRealTimers(); vi.restoreAllMocks();});

it("bounds requests, shares each requested identity once, and aborts remaining work", async () => {
  const tasks = [...DURABLE_TASK_IDS.map(id => summary(id)), summary("019a42b0-0000-7000-8000-000000000126")];
  const requests = tasks.map(() => Promise.withResolvers<Response>());
  const network = vi.fn<typeof fetch>().mockImplementation(() => requests[network.mock.calls.length - 1]!.promise);
  vi.stubGlobal("fetch", network);
  const controller = new AbortController();
  const observe = vi.fn();
  const loading = loadDurableTaskDetails("workspace", "fleet", tasks.map(currentDurableTaskRef), controller.signal, observe);
  expect(network).toHaveBeenCalledTimes(3);
  requests[0]!.resolve(Response.json(detail(tasks[0])));
  await vi.waitFor(() => expect(network).toHaveBeenCalledTimes(4));
  expect(observe).toHaveBeenCalledWith(currentDurableTaskRef(tasks[0]!), {kind: "ready", value: detail(tasks[0])});
  controller.abort();
  requests.slice(1).forEach((request, index) => request.resolve(Response.json(detail(tasks[index + 1]))));
  await loading;
  expect(observe.mock.calls.filter(([, result]) => result !== null)).toHaveLength(1);
  for (const [, init] of network.mock.calls) expect(init?.signal?.aborted).toBe(true);
  await loadDurableTaskDetails("workspace", "fleet", tasks.map(currentDurableTaskRef), controller.signal, observe);
  expect(network).toHaveBeenCalledTimes(4);
});

it("refuses mismatched task identity and older versions, then recovers transient responses automatically", async () => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0);
  const task = summary(DURABLE_TASK_IDS[0], "running", 2);
  const network = vi.fn<typeof fetch>().mockResolvedValueOnce(Response.json(detail(summary(DURABLE_TASK_IDS[1], "running", 2))))
    .mockResolvedValueOnce(Response.json(detail(summary(DURABLE_TASK_IDS[0], "running", 1))))
    .mockResolvedValueOnce(new Response(null, {status: 503})).mockResolvedValueOnce(Response.json(detail(task)));
  vi.stubGlobal("fetch", network);
  const observe = vi.fn();
  const controller = new AbortController();
  const loading = loadDurableTaskDetails("workspace", "fleet", [currentDurableTaskRef(task)], controller.signal, observe);
  await vi.advanceTimersByTimeAsync(0);
  expect(observe).toHaveBeenLastCalledWith(currentDurableTaskRef(task), {kind: "failed", reason: "invalid"});
  await vi.advanceTimersByTimeAsync(500);
  expect(observe).toHaveBeenLastCalledWith(currentDurableTaskRef(task), {kind: "failed", reason: "invalid"});
  await vi.advanceTimersByTimeAsync(1000);
  expect(observe).toHaveBeenLastCalledWith(currentDurableTaskRef(task), {kind: "failed", reason: "unreachable"});
  await vi.advanceTimersByTimeAsync(2000);
  await loading;
  expect(observe).toHaveBeenLastCalledWith(currentDurableTaskRef(task), {kind: "ready", value: detail(task)});
  expect(network).toHaveBeenCalledTimes(4);
});

it("does not retry access or missing responses and cancels pending recovery on scope disposal", async () => {
  vi.useFakeTimers();
  const network = vi.fn<typeof fetch>().mockResolvedValueOnce(new Response(null, {status: 403}))
    .mockResolvedValueOnce(new Response(null, {status: 404})).mockResolvedValueOnce(new Response(null, {status: 503}));
  vi.stubGlobal("fetch", network);
  const controller = new AbortController();
  const observe = vi.fn();
  await loadDurableTaskDetails("workspace", "fleet", DURABLE_TASK_IDS.slice(0, 2).map(id => currentDurableTaskRef(summary(id))), controller.signal, observe);
  expect(observe).toHaveBeenCalledWith(currentDurableTaskRef(summary(DURABLE_TASK_IDS[0])), {kind: "failed", reason: "access"});
  expect(observe).toHaveBeenCalledWith(currentDurableTaskRef(summary(DURABLE_TASK_IDS[1])), {kind: "failed", reason: "missing"});
  const pending = loadDurableTaskDetails("workspace", "fleet", [currentDurableTaskRef(summary())], controller.signal, observe);
  await vi.advanceTimersByTimeAsync(0);
  controller.abort();
  await pending;
  await vi.advanceTimersByTimeAsync(5000);
  expect(network).toHaveBeenCalledTimes(3);
});

it("exhausts each failed detail after four reads without retrying successful peers", async () => {
  vi.useFakeTimers(); vi.setSystemTime(0); vi.spyOn(Math, "random").mockReturnValue(0);
  const attempts: number[] = [];
  const network = vi.fn<typeof fetch>().mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(DURABLE_TASK_IDS[1])) return Response.json(detail(summary(DURABLE_TASK_IDS[1])));
    attempts.push(Date.now()); return new Response(null, {status: 503});
  });
  vi.stubGlobal("fetch", network);
  const observe = vi.fn();
  const pending = loadDurableTaskDetails("workspace", "fleet", DURABLE_TASK_IDS.slice(0, 2).map(id => currentDurableTaskRef(summary(id))), new AbortController().signal, observe);
  await vi.advanceTimersByTimeAsync(100000); await pending;
  expect(attempts).toEqual([0, 500, 1500, 3500]);
  expect(network).toHaveBeenCalledTimes(5);
  expect(observe).toHaveBeenLastCalledWith(currentDurableTaskRef(summary()), {kind: "failed", reason: "unreachable"});
  expect(observe).toHaveBeenCalledWith(currentDurableTaskRef(summary(DURABLE_TASK_IDS[1])), {kind: "ready", value: detail(summary(DURABLE_TASK_IDS[1]))});
});
