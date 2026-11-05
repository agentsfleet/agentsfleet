// @vitest-environment node
import { afterEach, expect, it, vi } from "vitest";
import { adoptTaskGoal, controlTask, readTask } from "@/lib/api/tasks";
import { detail, TASK_IDS } from "./helpers/task-reads";
const TAG = `"${"a".repeat(64)}"`;
afterEach(() => vi.unstubAllGlobals());

it("requires confirmed version and generation and never retries uncertain mutations", async () => {
  const task = {...detail(), etag: TAG};
  const signal = new AbortController().signal;
  const network = vi.fn<typeof fetch>().mockResolvedValue(Response.json(task, {headers: {etag: TAG}}));
  vi.stubGlobal("fetch", network);
  expect(await controlTask("w", "f", detail(), "paused", signal)).toEqual({kind: "failed", reason: "changed"});
  expect(network).not.toHaveBeenCalled();
  expect(await controlTask("w/1", "f?2", task, "paused", signal)).toEqual({kind: "confirmed", value: task});
  expect(network.mock.calls[0]?.[0]).toContain("w%2F1/fleets/f%3F2/tasks/");
  expect(network.mock.calls[0]?.[1]).toMatchObject({method: "PATCH", redirect: "manual", cache: "no-store",
    headers: {"If-Match": TAG}, body: JSON.stringify({state: "paused", expected_generation: 1})});
  network.mockResolvedValueOnce(Response.json(task, {headers: {etag: TAG}}));
  expect((await adoptTaskGoal("w", "f", task, TASK_IDS[1], TASK_IDS[2], signal)).kind).toBe("confirmed");
  expect(JSON.parse(String(network.mock.calls[1]?.[1]?.body))).toEqual({goal_revision_id: TASK_IDS[1], operation_id: TASK_IDS[2], expected_generation: 1});
});

it("keeps refusal, stale state, limits, malformed confirmation and transport uncertainty distinct", async () => {
  const redirect = new Response(null);
  Object.defineProperty(redirect, "type", {value: "opaqueredirect"});
  const replies = [new Response(null, {status: 401}), new Response(null, {status: 403}), redirect,
    ...[404, 409, 412, 428, 429, 503].map(status => new Response(null, {status})),
    Response.json(null), Response.json([]), Response.json("wrong"), Response.json({}), new Response("{invalid")];
  const network = vi.fn<typeof fetch>();
  for (const response of replies) network.mockResolvedValueOnce(response);
  network.mockRejectedValueOnce(new Error("lost acknowledgement"));
  vi.stubGlobal("fetch", network);
  const results = [];
  for (let i = 0; i < replies.length + 1; i++) results.push(await controlTask("w", "f", {...detail(), etag: TAG}, "cancelled", new AbortController().signal));
  expect(results).toEqual(["access", "access", "access", "changed", "changed", "changed", "changed", "limited", "unconfirmed", "invalid", "invalid", "invalid", "invalid", "unconfirmed", "unconfirmed"].map(reason => ({kind: "failed", reason})));
  expect(network).toHaveBeenCalledTimes(replies.length + 1);
});

it("accepts only GitHub repository links and validates tagged detail envelopes", async () => {
  const target = {kind: "existing_pr", repository: "a/b", number: 1, branch: "work", head: "a".repeat(40), tree: "b".repeat(40), workflows: []};
  const network = vi.fn<typeof fetch>();
  for (const url of ["https://github.com/a/b/pull/1", "https://evil.example/a/b"]) network.mockResolvedValueOnce(Response.json({...detail(), context: {...detail().context, target: {...target, url}}}, {headers: {etag: TAG}}));
  for (const value of [null, [], "wrong"]) network.mockResolvedValueOnce(Response.json(value));
  vi.stubGlobal("fetch", network);
  expect((await readTask("w", "f", TASK_IDS[0], new AbortController().signal)).kind).toBe("ready");
  for (let i = 0; i < 4; i++) expect(await readTask("w", "f", TASK_IDS[0], new AbortController().signal)).toEqual({kind: "failed", reason: "invalid"});
});
