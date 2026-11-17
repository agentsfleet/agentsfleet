// @vitest-environment node
// Native Date and AbortSignal behavior keeps malformed-deadline failures observable.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { watchManualApprovalCompletion } from "@/lib/auth/manual-approval-completion";

const NOW = Date.parse("2026-11-16T12:00:00Z");
const SESSION = "0190b3c0-1111-7000-8000-000000000001";
let fetchStatus: ReturnType<typeof vi.fn<() => Promise<Response>>>;
let owners: AbortController[];

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  vi.spyOn(Math, "random").mockReturnValue(0);
  fetchStatus = vi.fn<() => Promise<Response>>().mockImplementation(async () => new Response(null));
  vi.stubGlobal("fetch", fetchStatus);
  owners = [];
});

afterEach(() => {
  for (const owner of owners) owner.abort();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

function observe(owner = new AbortController()) {
  owners.push(owner);
  const update = vi.fn();
  const done = watchManualApprovalCompletion(SESSION, owner.signal, update);
  return { owner, update, done };
}

function consumed() { return Response.json({ error_code: "UZ-AUTH-012" }, { status: 410 }); }
async function tick(ms = 2_000) { await vi.advanceTimersByTimeAsync(ms); }

it("does not read or publish after ownership was already cancelled", async () => {
  const owner = new AbortController();
  owner.abort();
  const run = observe(owner);
  await run.done;
  expect(fetchStatus).not.toHaveBeenCalled();
  expect(run.update).not.toHaveBeenCalled();
});

it("stops when the renewed server deadline has expired", async () => {
  fetchStatus.mockResolvedValueOnce(Response.json({ status: "verification_pending", expires_at: new Date(NOW - 1).toISOString() }));
  const run = observe();
  await tick();
  await run.done;
  expect(fetchStatus).toHaveBeenCalledOnce();
  expect(run.update).toHaveBeenCalledWith({ kind: "failed", message: expect.stringContaining("expired") });
});

it("stops when the renewed server deadline expires during a poll wait", async () => {
  fetchStatus.mockResolvedValueOnce(Response.json({ status: "verification_pending", expires_at: new Date(NOW + 3_000).toISOString() }));
  const run = observe();
  await tick(3_000);
  await run.done;
  expect(fetchStatus).toHaveBeenCalledOnce();
  expect(run.update).toHaveBeenCalledWith({ kind: "failed", message: expect.stringContaining("expired") });
});

it("never extends its five-minute bound for a later server deadline", async () => {
  fetchStatus.mockImplementation(async () => Response.json({ status: "verification_pending", expires_at: new Date(NOW + 600_000).toISOString() }));
  const run = observe();
  await tick(300_000);
  await run.done;
  expect(fetchStatus).toHaveBeenCalledTimes(149);
  expect(run.update).toHaveBeenCalledOnce();
  expect(run.update).toHaveBeenCalledWith({ kind: "failed", message: expect.stringContaining("expired") });
});

it.each([
  { status: "verification_pending", expires_at: "invalid" },
  { status: "verification_pending", expires_at: "2026-11-16T12:00:00+00" },
  { status: "verification_pending", expires_at: "2026-11-16T12:00:00 +00:00" },
  { status: "verification_pending", expires_at: null },
  { status: "consumed", expires_at: new Date(NOW + 300_000).toISOString() },
  { error_code: "UZ-AUTH-012" },
])("does not trust malformed waiting status: %j", async body => {
  fetchStatus.mockResolvedValueOnce(Response.json(body)).mockImplementationOnce(async () => consumed());
  const run = observe();
  await tick();
  expect(run.update).not.toHaveBeenCalled();
  await tick();
  await run.done;
  expect(fetchStatus).toHaveBeenCalledTimes(2);
  expect(run.update).toHaveBeenCalledWith({ kind: "completed" });
});

it("cancels the pending wait without starting a status request", async () => {
  const run = observe();
  run.owner.abort();
  await run.done;
  expect(fetchStatus).not.toHaveBeenCalled();
  expect(run.update).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it("handles cancellation while a poll wait is being scheduled", async () => {
  const owner = new AbortController();
  vi.mocked(Math.random).mockImplementationOnce(() => { owner.abort(); return 0; });
  const run = observe(owner);
  await run.done;
  expect(fetchStatus).not.toHaveBeenCalled();
  expect(run.update).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it("caps reads even when a clock adjustment prevents the wall-clock deadline", async () => {
  vi.spyOn(Date, "now").mockReturnValue(NOW);
  const run = observe();
  await tick(300_000);
  await run.done;
  expect(fetchStatus).toHaveBeenCalledTimes(150);
  expect(run.update).toHaveBeenCalledOnce();
  expect(run.update).toHaveBeenCalledWith({ kind: "failed", message: expect.stringContaining("could not be confirmed") });
});

it.each([null, "redirect response"])("rejects a redirected status response with body %j", async body => {
  const response = new Response(body);
  Object.defineProperty(response, "redirected", { value: true });
  fetchStatus.mockResolvedValueOnce(response);
  const run = observe();
  await tick();
  await run.done;
  expect(fetchStatus).toHaveBeenCalledOnce();
  expect(run.update).toHaveBeenCalledWith({ kind: "failed", message: expect.any(String) });
});

it("rejects an invalid refusal schema without announcing completion", async () => {
  fetchStatus.mockResolvedValueOnce(Response.json({ error_code: 12 }, { status: 410 }));
  const run = observe();
  await tick();
  await run.done;
  expect(run.update).toHaveBeenCalledWith({ kind: "failed", message: expect.any(String) });
});

it.each([200, 429, 503])("retries a transient %i response without a body", async status => {
  fetchStatus.mockResolvedValueOnce(new Response(null, { status })).mockImplementationOnce(async () => consumed());
  const run = observe();
  await tick(4_000);
  await run.done;
  expect(fetchStatus).toHaveBeenCalledTimes(2);
  expect(run.update).toHaveBeenCalledWith({ kind: "completed" });
});

it("does not publish if ownership ends as a waiting body is discarded", async () => {
  const owner = new AbortController();
  const body = new ReadableStream({ cancel() { owner.abort(); } });
  fetchStatus.mockResolvedValueOnce(new Response(body, { status: 503 }));
  const run = observe(owner);
  await tick();
  await run.done;
  expect(body.locked).toBe(false);
  expect(run.update).not.toHaveBeenCalled();
});

it("cancels and releases an outstanding response reader even if cancellation rejects", async () => {
  const cancel = vi.fn().mockRejectedValue(new Error("stream cancellation failed"));
  const body = new ReadableStream({ cancel });
  fetchStatus.mockResolvedValueOnce(new Response(body, { status: 410 }));
  const run = observe();
  await tick();
  expect(body.locked).toBe(true);
  run.owner.abort();
  await run.done;
  expect(cancel).toHaveBeenCalledOnce();
  expect(body.locked).toBe(false);
  expect(run.update).not.toHaveBeenCalled();
});

it("releases a body when cancellation wins the fetch response race", async () => {
  const owner = new AbortController();
  const response = consumed();
  fetchStatus.mockImplementationOnce(async () => { owner.abort(); return response; });
  const run = observe(owner);
  await tick();
  await run.done;
  expect(response.body?.locked).toBe(false);
  expect(run.update).not.toHaveBeenCalled();
});

it("recovers from a failed response stream without leaking its reader", async () => {
  const body = new ReadableStream({ start(controller) { controller.error(new Error("stream failed")); } });
  fetchStatus.mockResolvedValueOnce(new Response(body, { status: 410 })).mockImplementationOnce(async () => consumed());
  const run = observe();
  await tick(4_000);
  await run.done;
  expect(body.locked).toBe(false);
  expect(run.update).toHaveBeenCalledWith({ kind: "completed" });
});
